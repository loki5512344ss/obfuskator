//! Spike, часть 2: доступна ли **генерация** кадров `StackMapTable`.
//!
//! Ключевой вопрос `docs/stack.md` §3: считает ли `ristretto_classfile` кадры
//! сам и отдаёт ли их наружу. Если да — наша работа сводится к записи; если
//! нет — кадры придётся считать (или вендорить их `verifiers::bytecode`).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use ristretto_classfile::attributes::{Attribute, Instruction, StackFrame};
use ristretto_classfile::verifiers::bytecode::{
    FallbackStrategy, VerificationPath, VerifierConfig, verify_class,
};
use ristretto_classfile::verifiers::context::VerificationContext;
use ristretto_classfile::verifiers::error::Result as VerifyResult;
use ristretto_classfile::{
    ClassAccessFlags, ClassFile, Constant, ConstantPool, MethodAccessFlags, Version,
};

/// Разрешающий контекст: в spike'е нас интересует только сам вывод типов.
#[derive(Debug)]
struct Permissive;

impl VerificationContext for Permissive {
    fn is_subclass(&self, _subclass: &str, _superclass: &str) -> VerifyResult<bool> {
        Ok(true)
    }

    fn is_assignable(&self, _target: &str, _source: &str) -> VerifyResult<bool> {
        Ok(true)
    }

    fn common_superclass(&self, _class1: &str, _class2: &str) -> VerifyResult<String> {
        Ok("java/lang/Object".to_string())
    }
}

/// `void run()` с одной ветвью: на сдвиге 3 (`Ifeq`-target) кадр обязателен.
fn branch_class() -> ClassFile<'static> {
    let mut constant_pool = ConstantPool::default();
    let utf8_name = constant_pool
        .add(Constant::utf8("demo/Branch"))
        .expect("utf8");
    let this_class = constant_pool
        .add(Constant::Class(utf8_name))
        .expect("class");
    let run = constant_pool.add(Constant::utf8("run")).expect("utf8");
    let descriptor = constant_pool.add(Constant::utf8("()V")).expect("utf8");
    let code = constant_pool.add(Constant::utf8("Code")).expect("utf8");

    ClassFile {
        version: Version::Java8 { minor: 0 },
        constant_pool,
        access_flags: ClassAccessFlags::PUBLIC,
        this_class,
        super_class: 0,
        interfaces: Vec::new(),
        fields: Vec::new(),
        methods: vec![ristretto_classfile::Method {
            access_flags: MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
            name_index: run,
            descriptor_index: descriptor,
            attributes: vec![Attribute::Code {
                name_index: code,
                max_stack: 1,
                max_locals: 0,
                code: vec![
                    Instruction::Iconst_1,
                    Instruction::Ifeq(3),
                    Instruction::Return,
                    Instruction::Return,
                ],
                exception_table: Vec::new(),
                attributes: Vec::new(),
            }],
        }],
        attributes: Vec::new(),
        code_source_url: None,
    }
}

fn inference_config() -> VerifierConfig {
    VerifierConfig::default().with_fallback_strategy(FallbackStrategy::AlwaysInference)
}

/// Без `StackMapTable` inference обязан пройти сам — значит, движок кадров у
/// крейта есть, просто его результат не отдаётся наружу.
#[test]
fn inference_path_verifies_without_stack_map_table() {
    let class_file = branch_class();
    let results = verify_class(&class_file, &Permissive, &inference_config()).expect("verify");

    assert_eq!(results.len(), 1);
    assert!(results[0].success, "ошибка: {:?}", results[0].error);
    assert_eq!(results[0].path_used, VerificationPath::Inference);
}

/// Строгий режим без кадров обязан упасть — иначе требование `StackMapTable`
/// для классов ≥ 50 не проверяется вовсе.
#[test]
fn strict_mode_rejects_missing_stack_map_table() {
    let class_file = branch_class();
    let config = VerifierConfig::default().with_fallback_strategy(FallbackStrategy::Strict);
    let outcome = verify_class(&class_file, &Permissive, &config);

    // Строгий режим падает либо `Err`, либо `success == false` — оба варианта
    // приемлемы, важен сам отказ.
    let rejected = match &outcome {
        Err(_) => true,
        Ok(results) => results.iter().any(|r| !r.success),
    };
    assert!(rejected, "строгий режим обязан отвергнуть класс без кадров");
}

/// Негативный результат spike'а: кадр не на том сдвиге не отдаёт диагностику.
///
/// Ждали `Ok(vec![result])` с заполненным `diagnostic.expected` — тогда
/// генерация кадров шла бы итеративным путём через публичный API. Фактически
/// `verify_class` в строгом режиме возвращает `Err(VerifyError)`, и ожидаемый
/// кадр до нас не доходит: из крейта достаётся только место ошибки
/// («merge point at offset N»), но не её содержимое.
///
/// Вывод для `docs/stack.md` §3: проверять кадры крейт умеет, отдавать — нет.
#[test]
fn diagnostic_exposes_expected_frame_on_failure() {
    let mut class_file = branch_class();
    let name_index = class_file
        .constant_pool
        .add_utf8("StackMapTable")
        .expect("utf8");
    // Кадр на сдвиге 0, а настоящий якорь — на сдвиге 3.
    class_file.attributes.push(Attribute::StackMapTable {
        name_index,
        frames: vec![StackFrame::SameFrame { frame_type: 0 }],
    });
    // Кладём атрибут в метод, а не в класс: `StackMapTable` живёт в `Code`.
    let stack_map = class_file.attributes.pop().expect("frame");
    match &mut class_file.methods[0].attributes[0] {
        Attribute::Code { attributes, .. } => attributes.push(stack_map),
        other => panic!("ожидался Code, получено {other:?}"),
    }

    let strict = VerifierConfig::default().with_fallback_strategy(FallbackStrategy::Strict);
    let outcome = verify_class(&class_file, &Permissive, &strict);

    // Наблюдаемое поведение: `Err`, а не `Ok` с `diagnostic`.
    let err = outcome.expect_err("битые кадры обязаны быть отвергнуты");
    let message = err.to_string();
    assert!(
        message.contains("StackMapTable"),
        "ожидалась ошибка про кадры, получено: {message}"
    );
}
