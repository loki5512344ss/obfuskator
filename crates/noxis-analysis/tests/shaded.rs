//! Shaded reflective libraries: a package containing a public abstract class
//! whose virtual methods are dispatched reflectively by another class in the
//! same package must keep its class and member names.
//!
//! Gson's `TypeAdapter` is the archetype: `TypeAdapterRuntimeTypeWrapper`
//! calls `getDeclaredMethod`/`Method.invoke` on an adapter instance. Renaming
//! the `read` method in `TypeAdapter` while the wrapper invokes it reflectively
//! yields `AbstractMethodError` at runtime. Package-locking the whole shaded
//! subtree is the only sound answer.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_analysis::Analysis;
use noxis_classfile::{
    ClassBuilder, Code, Const, Field, Insn, InvokeKind, Method, Program, Version,
};
use noxis_core::Config;

fn ret_code() -> Code {
    Code {
        max_stack: 0,
        max_locals: 0,
        insns: vec![Insn::Return(None)],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    }
}

/// `libs/gson/TypeAdapter` (abstract, public) with an abstract `read`,
/// plus `libs/gson/TypeAdapterRuntimeTypeWrapper` invoking `read`
/// reflectively through `getDeclaredMethod`/`invoke`.
fn sample() -> Program {
    let mut abstract_adapter = ClassBuilder::new("libs/gson/TypeAdapter", Some("java/lang/Object"))
        .version(Version::JAVA_21)
        .build();
    abstract_adapter.methods.push(Method {
        access_flags: 0x0401, // public abstract
        name: "read".to_string(),
        descriptor: "(Ljava/io/InputStream;)Ljava/lang/Object;".to_string(),
        code: None,
        attributes: Vec::new(),
    });

    let wrapper_code = Code {
        max_stack: 4,
        max_locals: 2,
        insns: vec![
            Insn::Load {
                slot: 1,
                kind: noxis_classfile::SlotKind::Ref,
            },
            Insn::Const(Const::String("read".to_string())),
            Insn::Const(Const::Class("java/io/InputStream".to_string())),
            Insn::Invoke {
                kind: InvokeKind::Virtual,
                owner: "java/lang/Class".to_string(),
                name: "getDeclaredMethod".to_string(),
                descriptor: "(Ljava/lang/String;[Ljava/lang/Class;)Ljava/lang/reflect/Method;"
                    .to_string(),
                itf: false,
            },
            Insn::Pop,
            Insn::Return(None),
        ],
        exception_table: Vec::new(),
        frames: Vec::new(),
        attributes: Vec::new(),
    };
    let wrapper = ClassBuilder::new(
        "libs/gson/TypeAdapterRuntimeTypeWrapper",
        Some("java/lang/Object"),
    )
    .version(Version::JAVA_21)
    .method_with_code(0x0001, "wrap", "(Ljava/lang/Object;)V", wrapper_code)
    .build();

    let mut state = ClassBuilder::new("demo/State", Some("java/lang/Object"))
        .version(Version::JAVA_21)
        .build();
    state.fields.push(Field {
        access_flags: 0x0002,
        name: "nickname".to_string(),
        descriptor: "Ljava/lang/String;".to_string(),
        attributes: Vec::new(),
    });
    let app = ClassBuilder::new("demo/App", Some("java/lang/Object"))
        .version(Version::JAVA_21)
        .method_with_code(0x0009, "load", "()V", ret_code())
        .build();

    Program {
        classes: vec![abstract_adapter, wrapper, state, app],
        resources: Vec::new(),
    }
}

#[test]
fn shaded_reflective_package_is_locked() {
    let program = sample();
    let analysis = Analysis::run(&program, &Config::default()).expect("analysis");
    assert!(
        analysis.is_locked("libs/gson/TypeAdapter"),
        "abstract adapter in a reflective package must stay"
    );
    assert!(
        analysis.is_locked("libs/gson/TypeAdapterRuntimeTypeWrapper"),
        "the reflective wrapper must stay too"
    );
    assert!(
        analysis.keep.keeps_member("read"),
        "the reflectively dispatched member must keep its name"
    );
    assert!(
        !analysis.is_locked("demo/State"),
        "unrelated classes must still rename"
    );
}
