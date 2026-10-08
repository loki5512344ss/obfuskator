//! Иерархия классов: наследование, переопределения, безопасность переименования.

use std::collections::{HashMap, HashSet};

use noxis_classfile::Program;

/// Узел иерархии: суперкласс и интерфейсы (по внутренним именам).
#[derive(Debug, Clone, Default)]
pub struct HierarchyNode {
    pub super_class: Option<String>,
    pub interfaces: Vec<String>,
}

/// Построенная иерархия классов.
#[derive(Debug, Clone, Default)]
pub struct ClassHierarchy {
    nodes: HashMap<String, HierarchyNode>,
}

impl ClassHierarchy {
    /// Построить иерархию из программы (только по имеющимся классам).
    pub fn build(program: &Program) -> Self {
        let mut nodes = HashMap::with_capacity(program.classes.len());
        for class in &program.classes {
            nodes.insert(
                class.this_class.clone(),
                HierarchyNode {
                    super_class: class.super_class.clone(),
                    interfaces: class.interfaces.clone(),
                },
            );
        }
        Self { nodes }
    }

    /// Прямой суперкласс (для LUB при выводе кадров).
    pub fn superclass(&self, internal_name: &str) -> Option<String> {
        self.nodes
            .get(internal_name)
            .and_then(|n| n.super_class.clone())
    }

    /// Есть ли класс в иерархии (в том числе из внешнего classpath — нет).
    pub fn contains(&self, internal_name: &str) -> bool {
        self.nodes.contains_key(internal_name)
    }

    /// Прямые предки: суперкласс + интерфейсы.
    pub fn parents(&self, internal_name: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(node) = self.nodes.get(internal_name) {
            out.extend(node.super_class.iter().cloned());
            out.extend(node.interfaces.iter().cloned());
        }
        out
    }

    /// Является ли `child` подклассом `parent` (в пределах программы).
    ///
    /// Неполная иерархия (внешние зависимости не грузим) — это осознанно:
    /// обфускатор работает по артефакту, а не по всему classpath.
    pub fn is_subclass_of(&self, child: &str, parent: &str) -> bool {
        if child == parent {
            return true;
        }
        let mut seen = HashSet::new();
        let mut stack = vec![child.to_string()];
        while let Some(current) = stack.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            for p in self.parents(&current) {
                if p == parent {
                    return true;
                }
                stack.push(p);
            }
        }
        false
    }

    /// Все классы программы, которые наследуют `parent` (включая его самого).
    pub fn descendants(&self, parent: &str) -> Vec<String> {
        self.nodes
            .keys()
            .filter(|name| self.is_subclass_of(name, parent))
            .cloned()
            .collect()
    }

    /// Классы, объявляющие метод с данным именем/дескриптором, вверх по цепочке.
    pub fn declaring_chain(
        &self,
        program: &Program,
        start: &str,
        name: &str,
        descriptor: &str,
    ) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![start.to_string()];
        while let Some(current) = stack.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            if let Some(class) = program.class(&current)
                && class.method(name, descriptor).is_some()
            {
                out.push(current.clone());
            }
            stack.extend(self.parents(&current));
        }
        out
    }
}

impl noxis_classfile::Hierarchy for ClassHierarchy {
    fn superclass(&self, internal_name: &str) -> Option<String> {
        self.superclass(internal_name)
    }
}
/// Почему метод нельзя/можно переименовывать (аналог `InheritanceAnalyzer`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameSafety {
    /// Переименовывать можно: это точка, где имя не видно снаружи.
    Safe,
    /// Конструктор/статический блок: имя зафиксировано форматом JVM.
    FixedName,
    /// Виртуальный метод, который может переопределяться вне артефакта.
    VirtualExternal,
    /// Рефлексия/сериализация может обратиться по имени.
    ReferencedByName,
    /// Метод не найден в классе.
    Missing,
}

/// Оценка безопасности переименования метода.
///
/// Правила (сознательно консервативные, как в `OBF/InheritanceAnalyzer`):
/// - `<init>`/`<clinit>` — никогда;
/// - `private`/`static`/`final`/`<package-private>` внутри финального класса — можно;
/// - `public`/`protected`/`package-private`, если класс не `final` и есть
///   наследники (внутри артефакта или нет) — нельзя, пока нет доказательства.
pub fn method_rename_safety(
    program: &Program,
    hierarchy: &ClassHierarchy,
    owner: &str,
    name: &str,
    descriptor: &str,
    externally_referenced: bool,
) -> RenameSafety {
    if name == "<init>" || name == "<clinit>" {
        return RenameSafety::FixedName;
    }
    let Some(class) = program.class(owner) else {
        return RenameSafety::Missing;
    };
    if externally_referenced {
        return RenameSafety::ReferencedByName;
    }
    let Some(method) = class.method(name, descriptor) else {
        return RenameSafety::Missing;
    };

    const ACC_PRIVATE: u16 = 0x0002;
    const ACC_STATIC: u16 = 0x0008;
    const ACC_FINAL: u16 = 0x0010;
    const ACC_PUBLIC: u16 = 0x0001;
    const ACC_PROTECTED: u16 = 0x0004;

    let flags = method.access_flags;
    if flags & ACC_PRIVATE != 0 || flags & ACC_STATIC != 0 || flags & ACC_FINAL != 0 {
        return RenameSafety::Safe;
    }

    // Не-private без `final`: ищем переопределения в иерархии.
    let chain = hierarchy.declaring_chain(program, owner, name, descriptor);
    let has_subclasses = chain
        .iter()
        .any(|c| c != owner && hierarchy.is_subclass_of(c, owner));
    if has_subclasses {
        return RenameSafety::VirtualExternal;
    }
    if flags & (ACC_PUBLIC | ACC_PROTECTED) != 0 {
        // Публичный метод финального/листьевого класса: внутри артефакта
        // переопределений нет, но снаружи могут зваться по имени.
        return RenameSafety::VirtualExternal;
    }
    RenameSafety::Safe
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;
    use noxis_classfile::ClassBuilder;

    fn sample_program() -> Program {
        let base = ClassBuilder::new("a/Base", None)
            .method_with_code(0x0001, "open", "()V", noxis_classfile::empty_code(0))
            .method_with_code(0x0002, "hidden", "()V", noxis_classfile::empty_code(0))
            .method_with_code(0x0001, "<init>", "()V", noxis_classfile::empty_code(0))
            .build();
        let derived = ClassBuilder::new("a/Der", Some("a/Base"))
            .method_with_code(0x0001, "open", "()V", noxis_classfile::empty_code(0))
            .build();
        Program {
            classes: vec![base, derived],
            resources: Vec::new(),
        }
    }

    #[test]
    fn hierarchy_links_superclass() {
        let program = sample_program();
        let h = ClassHierarchy::build(&program);
        assert!(h.is_subclass_of("a/Der", "a/Base"));
        assert!(!h.is_subclass_of("a/Base", "a/Der"));
    }

    #[test]
    fn private_method_is_safe() {
        let program = sample_program();
        let h = ClassHierarchy::build(&program);
        assert_eq!(
            method_rename_safety(&program, &h, "a/Base", "hidden", "()V", false),
            RenameSafety::Safe
        );
    }

    #[test]
    fn constructor_is_never_renamed() {
        let program = sample_program();
        let h = ClassHierarchy::build(&program);
        assert_eq!(
            method_rename_safety(&program, &h, "a/Base", "<init>", "()V", false),
            RenameSafety::FixedName
        );
    }

    #[test]
    fn virtual_method_with_subclass_is_unsafe() {
        let program = sample_program();
        let h = ClassHierarchy::build(&program);
        assert_eq!(
            method_rename_safety(&program, &h, "a/Base", "open", "()V", false),
            RenameSafety::VirtualExternal
        );
    }
}
