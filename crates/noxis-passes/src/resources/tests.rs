//! Тесты пасса ресурсов: границы совпадений и патч содержимого.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use noxis_classfile::{Program, Resource};
use noxis_core::{Config, Context, Pass};

use super::{ResourcesPass, is_text, remap_text};

fn mapping() -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    map.insert("org/example/Foo".to_string(), "a/b/C".to_string());
    map.insert("org/example/FooBar".to_string(), "a/b/D".to_string());
    map
}

#[test]
fn descriptor_form_is_replaced() {
    let text = "Lorg/example/Foo;  [Lorg/example/FooBar;";
    assert_eq!(
        remap_text(text, &mapping()),
        "La/b/C;  [La/b/D;",
        "дескрипторный вид обязан заменяться без проверки границ"
    );
}

#[test]
fn internal_form_respects_boundaries() {
    let map = mapping();
    assert_eq!(remap_text("owner=Lorg/example/Foo;", &map), "owner=La/b/C;");
    assert_eq!(
        remap_text("myorg/example/Foo", &map),
        "myorg/example/Foo",
        "совпадение внутри более длинного пути не трогаем"
    );
    assert_eq!(
        remap_text("org/example/FooBaz", &map),
        "org/example/FooBaz",
        "суффикс не должен совпасть с префиксом"
    );
}

#[test]
fn dotted_form_respects_boundaries() {
    let map = mapping();
    assert_eq!(
        remap_text("\"org.example.Foo\" => \"x\"", &map),
        "\"a.b.C\" => \"x\""
    );
    assert_eq!(
        remap_text("myorg.example.Foo", &map),
        "myorg.example.Foo",
        "точка внутри имени не является границей"
    );
}

#[test]
fn longest_name_wins() {
    assert_eq!(
        remap_text("Lorg/example/FooBar;", &mapping()),
        "La/b/D;",
        "длинное имя обрабатывается раньше короткого"
    );
}

#[test]
fn empty_map_is_a_noop() {
    let text = "Lorg/example/Foo; и org.example.Foo";
    assert_eq!(remap_text(text, &Default::default()), text);
}

#[test]
fn unicode_text_survives_unrelated_substitution() {
    let text = "комментарий: Лорем_ипсум — org/example/Bar";
    assert_eq!(remap_text(text, &mapping()), text);
}

#[test]
fn text_detection_covers_known_formats() {
    assert!(is_text("fabric.mod.json"));
    assert!(is_text("demo.mixins.json"));
    assert!(is_text("META-INF/services/org.example.Loader"));
    assert!(is_text("mod.accesswidener"));
    assert!(is_text("META-INF/versions/1.20/mod.json"));

    assert!(!is_text("assets/tex.png"));
    assert!(!is_text("lib/native.so"));
    assert!(!is_text("META-INF/services/"));
}

fn context_with_mapping() -> Context {
    let mut ctx = Context::new(Config {
        seed: Some(3),
        ..Config::default()
    });
    ctx.mapping
        .classes
        .insert("org/example/Foo".into(), "a/b/C".into());
    ctx
}

#[test]
fn pass_patches_text_and_counts_the_rest() {
    let mut program = Program {
        classes: Vec::new(),
        resources: vec![
            Resource {
                path: "fabric.mod.json".into(),
                data: br#"{"entrypoints":{"main":["org.example.Foo"]}}"#.to_vec(),
            },
            Resource {
                path: "META-INF/services/org.example.Foo".into(),
                data: b"# keep\n".to_vec(),
            },
            Resource {
                path: "assets/icon.png".into(),
                data: vec![0x89, 0x50, 0x4E, 0x47],
            },
        ],
    };

    let mut ctx = context_with_mapping();
    let report = ResourcesPass
        .apply(&mut ctx, &mut program)
        .expect("resources");

    assert_eq!(report.transformed, 1, "патчим только JSON");
    assert_eq!(report.skipped, 2);

    let json = std::str::from_utf8(&program.resources[0].data).unwrap();
    assert!(
        json.contains("a.b.C"),
        "binary-имя в JSON не заменено: {json}"
    );
    assert!(!json.contains("org.example.Foo"), "старое имя осталось");
    assert_eq!(program.resources[2].data, vec![0x89, 0x50, 0x4E, 0x47]);
}

#[test]
fn pass_is_a_noop_without_a_mapping() {
    let mut program = Program {
        classes: Vec::new(),
        resources: vec![Resource {
            path: "fabric.mod.json".into(),
            data: br#"{"x":"org.example.Foo"}"#.to_vec(),
        }],
    };

    let mut ctx = Context::new(Config::default());
    let report = ResourcesPass
        .apply(&mut ctx, &mut program)
        .expect("resources");

    assert_eq!(report.transformed, 0);
    assert_eq!(report.skipped, 1);
    assert!(
        !report.notes.is_empty(),
        "должна быть заметка о пустом маппинге"
    );
}
