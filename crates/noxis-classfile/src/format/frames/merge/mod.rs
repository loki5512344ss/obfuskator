//! Слияние типов (LUB) и разбор дескрипторов.
//!
//! Решётка: `Top` — «пусто» (берётся сосед), `Null` впитывается в ссылку,
//! одинаковые — сами в себя, ссылки — наименьший общий предок по иерархии.
//! Несочетаемые инициализированные типы (`int` vs ссылка после `astore`
//! в handler'е) дают `Top`, как обрезанные кадры `javac`: `Top` в кадре
//! принимает любое фактическое значение, поэтому это всегда sound.
//! Исключение — неинициализированные типы: их слияние означает сломанный
//! поток конструирования, здесь честная ошибка, а не молчаливый `Top`.

pub(super) mod descs;

use super::VerificationType;
use super::xfer::State;
use crate::error::ClassfileError;
use crate::model::Method;

use descs::write_local;

/// Иерархия классов для LUB: достаточно цепочки суперклассов.
pub trait Hierarchy {
    /// Прямой суперкласс (`None` — `java/lang/Object` или неизвестный).
    fn superclass(&self, class: &str) -> Option<String>;
}

/// Контекст слияния: имена для ошибок + иерархия для LCA.
pub(super) struct Merger<'h> {
    pub class: String,
    pub method: String,
    pub hierarchy: &'h dyn Hierarchy,
}

impl Merger<'_> {
    pub fn invalid(&self, msg: &str) -> ClassfileError {
        ClassfileError::InvalidCode {
            class: self.class.clone(),
            msg: format!("{}: {msg}", self.method),
        }
    }

    /// Влить входящее состояние; `true` — слот изменился.
    pub fn merge_into(
        &self,
        slot: &mut Option<State>,
        incoming: State,
    ) -> Result<bool, ClassfileError> {
        match slot {
            None => {
                *slot = Some(incoming);
                Ok(true)
            }
            Some(old) => {
                let merged = self.lub_states(old, &incoming)?;
                if *old == merged {
                    Ok(false)
                } else {
                    *old = merged;
                    Ok(true)
                }
            }
        }
    }

    /// Начальное состояние: параметры (+ `this`) поверх `Top`.
    pub fn initial(&self, method: &Method, max_locals: u16) -> Result<State, ClassfileError> {
        let is_static = method.access_flags & 0x0008 != 0;
        let (params, _) =
            descs::method_slots(&method.descriptor).map_err(|e| self.invalid(&e.0))?;
        let mut locals = vec![VerificationType::Top; max_locals as usize];
        let mut slot = 0;
        if !is_static {
            let this = if method.name == "<init>" {
                VerificationType::UninitializedThis
            } else {
                VerificationType::Object(self.class.clone())
            };
            slot = write_local(&mut locals, slot, vec![this])
                .map_err(|_| self.invalid("параметры не влезают в locals"))?;
        }
        for param in params {
            slot = write_local(&mut locals, slot, param)
                .map_err(|_| self.invalid("параметры не влезают в locals"))?;
        }
        Ok(State {
            locals,
            stack: Vec::new(),
        })
    }

    fn lub_states(&self, first: &State, second: &State) -> Result<State, ClassfileError> {
        if first.locals.len() != second.locals.len() {
            return Err(self.invalid("разная длина locals при слиянии"));
        }
        if first.stack.len() != second.stack.len() {
            return Err(self.invalid("разная глубина стека при слиянии"));
        }
        Ok(State {
            locals: first
                .locals
                .iter()
                .zip(&second.locals)
                .map(|(a, b)| self.lub_types(a, b))
                .collect::<Result<_, _>>()?,
            stack: first
                .stack
                .iter()
                .zip(&second.stack)
                .map(|(a, b)| self.lub_types(a, b))
                .collect::<Result<_, _>>()?,
        })
    }

    fn lub_types(
        &self,
        first: &VerificationType,
        second: &VerificationType,
    ) -> Result<VerificationType, ClassfileError> {
        use VerificationType::{Double, Float, Integer, Long, Null, Object, Top};
        if first == second {
            return Ok(first.clone());
        }
        match (first, second) {
            (Top, other) | (other, Top) => Ok(other.clone()),
            (Null, other @ Object(_)) | (other @ Object(_), Null) => Ok(other.clone()),
            (Null, Null) => Ok(Null),
            (Integer, Integer) | (Float, Float) | (Long, Long) | (Double, Double) => {
                Ok(first.clone())
            }
            (Object(a), Object(b)) => Ok(Object(self.common_superclass(a, b))),
            // Одинаковые уже разобраны выше; здесь неинициализированные
            // всегда разные — это сломанный поток конструирования.
            (VerificationType::UninitializedThis | VerificationType::Uninitialized(_), _)
            | (_, VerificationType::UninitializedThis | VerificationType::Uninitialized(_)) => {
                Err(self.invalid("слияние неинициализированных типов"))
            }
            // Несочетаемые инициализированные (например, `int` из тела `try`
            // и `Throwable` из handler'а в одном слоте): `Top`, как у `javac`.
            _ => Ok(Top),
        }
    }

    /// Наименьший общий предок: подъём по цепочкам, запасной — `Object`.
    fn common_superclass(&self, first: &str, second: &str) -> String {
        if first == second {
            return first.to_string();
        }
        if descs::split_array(first).is_some() || descs::split_array(second).is_some() {
            return self.array_lca(first, second);
        }
        let mut chain = Vec::new();
        let mut current = Some(first.to_string());
        let mut guard = 0;
        while let Some(name) = current {
            if name == second {
                return name;
            }
            chain.push(name.clone());
            current = self.hierarchy.superclass(&name);
            guard += 1;
            if guard > 1024 {
                break;
            }
        }
        let mut current = Some(second.to_string());
        guard = 0;
        while let Some(name) = current {
            if chain.contains(&name) {
                return name;
            }
            current = self.hierarchy.superclass(&name);
            guard += 1;
            if guard > 1024 {
                break;
            }
        }
        "java/lang/Object".to_string()
    }

    /// LCA массивов: та же размерность + LCA элементов, иначе `Object`.
    fn array_lca(&self, first: &str, second: &str) -> String {
        let (dims_a, elem_a) = descs::split_array(first).unwrap_or((0, first));
        let (dims_b, elem_b) = descs::split_array(second).unwrap_or((0, second));
        if dims_a == 0 || dims_b == 0 || dims_a != dims_b {
            return "java/lang/Object".to_string();
        }
        if elem_a == elem_b {
            return first.to_string();
        }
        if elem_a.len() == 1 || elem_b.len() == 1 {
            return "java/lang/Object".to_string();
        }
        let common = self.common_superclass(
            elem_a.strip_prefix('L').unwrap_or(elem_a),
            elem_b.strip_prefix('L').unwrap_or(elem_b),
        );
        format!("{}L{common};", "[".repeat(dims_a))
    }
}
