//! Type merging (LUB) and descriptor parsing.
//!
//! The lattice: `Top` is "empty" (the neighbour wins), `Null` is absorbed into a
//! reference, equal types map to themselves, references take the least upper
//! bound over the hierarchy. Incompatible initialized types (`int` vs a reference
//! after `astore` in a handler) yield `Top`, like `javac`'s truncated frames: a
//! `Top` in a frame accepts any actual value, so this is always sound. The
//! exception is uninitialized types: merging them means a broken construction
//! flow — here an honest error, not a silent `Top`.

pub(super) mod descs;

use super::VerificationType;
use super::xfer::State;
use crate::error::ClassfileError;
use crate::model::Method;

use descs::write_local;

/// The class hierarchy for LUB: the superclass chain is enough.
pub trait Hierarchy {
    /// The direct superclass (`None` — `java/lang/Object` or unknown).
    fn superclass(&self, class: &str) -> Option<String>;
}

/// Merge context: names for errors + hierarchy for LCA.
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

    /// Merge an incoming state; `true` — the slot changed.
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

    /// The initial state: parameters (+ `this`) over `Top`.
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
            // Equal ones are handled above; here uninitialized are always
            // different — that's a broken construction flow.
            (VerificationType::UninitializedThis | VerificationType::Uninitialized(_), _)
            | (_, VerificationType::UninitializedThis | VerificationType::Uninitialized(_)) => {
                Err(self.invalid("слияние неинициализированных типов"))
            }
            // Incompatible initialized (e.g. `int` from the `try` body and
            // `Throwable` from a handler in one slot): `Top`, like `javac`.
            _ => Ok(Top),
        }
    }

    /// Least common ancestor: climb the chains, fallback — `Object`.
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

    /// Arrays' LCA: same dimensionality + element LCA, otherwise `Object`.
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
