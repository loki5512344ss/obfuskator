//! Entrypoint'ы и точки, где имя класса/метода видно снаружи.

use std::collections::BTreeSet;

use noxis_classfile::Program;
use noxis_classfile::insn::{Const, Insn};

/// Всё, что нельзя переименовывать «как есть» и что нужно держать в keep-сете.
#[derive(Debug, Clone, Default)]
pub struct Entrypoints {
    /// Классы с `public static void main(String[])`.
    pub main_classes: BTreeSet<String>,
    /// `native`-методы: имя связано с JNI-схемой (`Java_pkg_Class_method`).
    pub native_methods: BTreeSet<(String, String, String)>,
    /// Интерфейсы из `META-INF/services/*`.
    pub service_interfaces: BTreeSet<String>,
    /// Имена, встречающиеся в рефлексивных вызовах (`Class.forName` и т.п.).
    pub reflective_names: BTreeSet<String>,
    /// Имена членов (`getMethod`/`getField`-литералы): их нельзя переименовывать.
    pub reflective_member_names: BTreeSet<String>,
    /// Поля с флагом `transient`+`serialVersionUID`-контекст: сериализуемые классы
    /// обязаны сохранить имя класса и полей (иначе ломается Java-сериализация).
    pub serializable_classes: BTreeSet<String>,
}

impl Entrypoints {
    /// Собрать entrypoint'ы по программе (без keep-правил - их добавляет [`Self::merge`]).
    pub fn collect(program: &Program) -> Self {
        let mut out = Self::default();

        for class in &program.classes {
            if class.this_class == "java/lang/Object" {
                continue;
            }
            if implements_serializable(program, &class.this_class) {
                out.serializable_classes.insert(class.this_class.clone());
            }
            for method in &class.methods {
                const ACC_STATIC: u16 = 0x0008;
                const ACC_NATIVE: u16 = 0x0100;
                if method.access_flags & ACC_NATIVE != 0 {
                    out.native_methods.insert((
                        class.this_class.clone(),
                        method.name.clone(),
                        method.descriptor.clone(),
                    ));
                }
                if method.name == "main"
                    && method.descriptor == "([Ljava/lang/String;)V"
                    && method.access_flags & ACC_STATIC != 0
                {
                    out.main_classes.insert(class.this_class.clone());
                }
                if let Some(code) = &method.code {
                    collect_reflective_names(&mut out, code);
                }
            }
        }

        out.service_interfaces = collect_service_interfaces(program);
        out
    }

    /// Объединить с другим набором (например, из keep-правил конфига).
    pub fn merge(&mut self, other: &Entrypoints) {
        self.main_classes.extend(other.main_classes.iter().cloned());
        self.native_methods
            .extend(other.native_methods.iter().cloned());
        self.service_interfaces
            .extend(other.service_interfaces.iter().cloned());
        self.reflective_names
            .extend(other.reflective_names.iter().cloned());
        self.reflective_member_names
            .extend(other.reflective_member_names.iter().cloned());
        self.serializable_classes
            .extend(other.serializable_classes.iter().cloned());
    }

    /// Класс, который нельзя переименовывать.
    pub fn keeps_class(&self, internal_name: &str) -> bool {
        self.main_classes.contains(internal_name)
            || self.serializable_classes.contains(internal_name)
            || self.reflective_names.contains(internal_name)
    }

    /// Член (метод/поле), который нельзя переименовывать.
    pub fn keeps_member(&self, name: &str) -> bool {
        self.reflective_member_names.contains(name)
    }
}

/// `implements java.io.Serializable` (по цепочке предков, в пределах программы).
fn implements_serializable(program: &Program, start: &str) -> bool {
    let mut stack = vec![start.to_string()];
    let mut seen = std::collections::HashSet::new();
    while let Some(name) = stack.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        if name == "java/io/Serializable" {
            return true;
        }
        let Some(class) = program.class(&name) else {
            // Внешний предок: считаем, что не знаем - консервативно не keep'им.
            continue;
        };
        if let Some(sup) = &class.super_class {
            stack.push(sup.clone());
        }
        stack.extend(class.interfaces.iter().cloned());
    }
    false
}

/// `META-INF/services/<iface>` → внутреннее имя интерфейса.
fn collect_service_interfaces(program: &Program) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for resource in &program.resources {
        let Some(name) = resource.path.strip_prefix("META-INF/services/") else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        out.insert(name.replace('.', "/"));
    }
    out
}

/// Линейный эвристический поиск имён в рефлексивных API.
///
/// Полноценный dataflow не нужен: если рядом с вызовом `Class.forName(...)` /
/// `getMethod(...)` лежит строковый литерал - это имя видно статически,
/// значит, его нельзя переименовывать.
fn collect_reflective_names(out: &mut Entrypoints, code: &noxis_classfile::Code) {
    /// API, чьи аргументы - **имена классов**.
    const CLASS_API: &[(&str, &str)] = &[
        ("java/lang/Class", "forName"),
        ("java/lang/ClassLoader", "loadClass"),
    ];
    /// API, чьи аргументы - **имена членов** (поле/метод).
    const MEMBER_API: &[(&str, &str)] = &[
        ("java/lang/Class", "getMethod"),
        ("java/lang/Class", "getDeclaredMethod"),
        ("java/lang/Class", "getField"),
        ("java/lang/Class", "getDeclaredField"),
    ];

    let mut last_string: Option<&str> = None;
    for insn in &code.insns {
        if let Insn::Const(Const::String(s)) = insn {
            last_string = Some(s.as_str());
            continue;
        }
        if let Insn::Invoke { owner, name, .. } = insn {
            let is_class_api = CLASS_API.iter().any(|(o, n)| o == owner && n == name);
            let is_member_api = MEMBER_API.iter().any(|(o, n)| o == owner && n == name);
            if let (true, Some(s)) = (is_class_api, last_string) {
                out.reflective_names.insert(s.replace('.', "/"));
            }
            if let (true, Some(s)) = (is_member_api, last_string) {
                out.reflective_member_names.insert(s.to_string());
            }
            last_string = None;
            continue;
        }
        // Инструкции, не относящиеся к строке/вызову, обрывают «соседность».
        if !matches!(insn, Insn::Mark(_) | Insn::Nop) {
            last_string = None;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;
    use noxis_classfile::insn::InvokeKind;
    use noxis_classfile::{ClassBuilder, Insn, empty_code};

    #[test]
    fn finds_main_and_native() {
        let main = ClassBuilder::new("app/Main", None)
            .method_with_code(0x0009, "main", "([Ljava/lang/String;)V", empty_code(1))
            .build();
        let mut native_class = ClassBuilder::new("app/Jni", None).build();
        native_class.methods.push(noxis_classfile::Method {
            access_flags: 0x0100, // ACC_NATIVE
            name: "ping".into(),
            descriptor: "()I".into(),
            code: None,
            attributes: Vec::new(),
        });
        let program = Program {
            classes: vec![main, native_class],
            resources: Vec::new(),
        };
        let ep = Entrypoints::collect(&program);
        assert!(ep.main_classes.contains("app/Main"));
        assert!(ep.native_methods.contains(&(
            "app/Jni".to_string(),
            "ping".to_string(),
            "()I".to_string()
        )));
        assert!(ep.keeps_class("app/Main"));
    }

    #[test]
    fn collects_service_interface() {
        let program = Program {
            classes: Vec::new(),
            resources: vec![noxis_classfile::Resource {
                path: "META-INF/services/com.example.Plugin".into(),
                data: b"app.Impl\n".to_vec(),
            }],
        };
        let ep = Entrypoints::collect(&program);
        assert!(ep.service_interfaces.contains("com/example/Plugin"));
    }

    #[test]
    fn picks_reflective_literal() {
        let mut code = empty_code(1);
        code.insns = vec![
            Insn::Const(Const::String("app.Secret".into())),
            Insn::Invoke {
                kind: InvokeKind::Static,
                owner: "java/lang/Class".into(),
                name: "forName".into(),
                descriptor: "(Ljava/lang/String;)Ljava/lang/Class;".into(),
                itf: false,
            },
        ];
        let class = ClassBuilder::new("app/Caller", None)
            .method_with_code(0x0009, "go", "()V", code)
            .build();
        let program = Program {
            classes: vec![class],
            resources: Vec::new(),
        };
        let ep = Entrypoints::collect(&program);
        assert!(ep.reflective_names.contains("app/Secret"));
    }
}
