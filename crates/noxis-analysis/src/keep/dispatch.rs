//! Внешний диспатч: семейства методов, которые программный код зовёт через
//! внешние интерфейсы/классы (не из артефакта). Их нельзя переименовывать -
//! внешний вызывающий код не переписать, и JVM получит `AbstractMethodError`.

use std::collections::{BTreeSet, HashSet};

use noxis_classfile::{Insn, InvokeKind, Program};

use crate::hierarchy::ClassHierarchy;

/// Семейства `(name, params)`, которые нельзя переименовывать: программа
/// вызывает их виртуально/интерфейсно на внешнем владельце, а некоторый
/// класс в предках этого владельца (внутри артефакта) объявляет то же
/// `(name, params)`. Переименование разорвало бы диспатч через внешний
/// интерфейс/класс - JVM упала бы с `AbstractMethodError ... is abstract`
/// (реальный случай: шадоуed gson `LinkedTreeMap$EntrySet$1`, который
/// JDK `Map.forEach` зовёт через `invokeinterface Iterator.hasNext`).
pub fn external_dispatch_families(
    program: &Program,
    hierarchy: &ClassHierarchy,
) -> BTreeSet<(String, String)> {
    let in_program: HashSet<&str> = program
        .classes
        .iter()
        .map(|c| c.this_class.as_str())
        .collect();

    // Виртуальные/интерфейсные вызовы на внешнем владельце (дедуплицированно).
    let mut external_calls: BTreeSet<(String, String, String)> = BTreeSet::new();
    for class in &program.classes {
        for method in &class.methods {
            let Some(code) = &method.code else { continue };
            for insn in &code.insns {
                if let Insn::Invoke {
                    kind,
                    owner,
                    name,
                    descriptor,
                    ..
                } = insn
                    && matches!(kind, InvokeKind::Virtual | InvokeKind::Interface)
                    && !in_program.contains(owner.as_str())
                    && let Some(params) = params_part(descriptor)
                {
                    external_calls.insert((owner.clone(), name.clone(), params.to_string()));
                }
            }
        }
    }
    if external_calls.is_empty() {
        return BTreeSet::new();
    }

    // Для каждого класса - внешние предки, достижимые сквозь внутренние
    // суперклассы/интерфейсы (цепочка обрывается на внешнем узле).
    let mut locked: BTreeSet<(String, String)> = BTreeSet::new();
    for class in &program.classes {
        let mut externals: HashSet<String> = HashSet::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut stack = hierarchy.parents(&class.this_class);
        while let Some(name) = stack.pop() {
            if in_program.contains(name.as_str()) {
                if seen.insert(name.clone()) {
                    stack.extend(hierarchy.parents(&name));
                }
            } else {
                externals.insert(name);
            }
        }
        for (owner, name, params) in &external_calls {
            if externals.contains(owner) {
                locked.insert((name.clone(), params.clone()));
            }
        }
    }
    locked
}

/// `(<params>)<ret>` → `(<params>)`, или `None` при кривом дескрипторе.
fn params_part(descriptor: &str) -> Option<&str> {
    let close = descriptor.find(')')?;
    if !descriptor.starts_with('(') {
        return None;
    }
    Some(&descriptor[..=close])
}
