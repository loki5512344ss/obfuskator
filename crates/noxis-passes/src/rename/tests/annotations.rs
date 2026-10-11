//! Annotation rewriting: descriptors follow the mapping, values do not.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::run_rename;
use noxis_classfile::{Annotation, AnnotationValue, Attribute, ClassBuilder, Version};

/// Class references inside annotations live as descriptors: a stale one
/// makes the JVM's reflection parser fail (`AnnotationFormatError`) or
/// resolve a ghost type. String *values* are data and stay put.
#[test]
fn annotation_descriptors_follow_the_mapping() {
    let mut program = super::sample();
    program.classes.push(
        ClassBuilder::new("app/Annotated", Some("java/lang/Object"))
            .version(Version::JAVA_21)
            .method_with_code(0x000a, "go", "()V", noxis_classfile::empty_code(0))
            .build(),
    );
    let annotated = program
        .classes
        .iter_mut()
        .find(|c| c.this_class == "app/Annotated")
        .expect("annotated");
    annotated
        .attributes
        .push(Attribute::RuntimeVisibleAnnotations(vec![Annotation {
            descriptor: "Lapp/Helper;".to_string(),
            elements: vec![
                (
                    "kind".to_string(),
                    AnnotationValue::Enum {
                        descriptor: "Lapp/Helper;".to_string(),
                        name: "ONE".to_string(),
                    },
                ),
                (
                    "klass".to_string(),
                    AnnotationValue::Class("Lapp/Helper;".to_string()),
                ),
                (
                    "nested".to_string(),
                    AnnotationValue::Nested(Box::new(Annotation {
                        descriptor: "Lapp/Helper;".to_string(),
                        elements: vec![(
                            "note".to_string(),
                            AnnotationValue::String("app/Helper".to_string()),
                        )],
                    })),
                ),
                (
                    "arr".to_string(),
                    AnnotationValue::Array(vec![AnnotationValue::Class(
                        "[Lapp/Helper;".to_string(),
                    )]),
                ),
            ],
        }]));
    let ctx = run_rename(&mut program);
    let helper = ctx.mapping.class("app/Helper").expect("renamed");
    let annotated_name = ctx.mapping.class("app/Annotated").expect("annotated");
    let class = program.class(annotated_name).expect("class");
    let annos = class
        .attributes
        .iter()
        .find_map(|a| match a {
            Attribute::RuntimeVisibleAnnotations(list) => Some(list),
            _ => None,
        })
        .expect("annotations");
    let anno = &annos[0];
    assert_eq!(anno.descriptor, format!("L{helper};"));
    let expect_desc = format!("L{helper};");
    assert_eq!(
        anno.elements[0].1,
        AnnotationValue::Enum {
            descriptor: expect_desc.clone(),
            name: "ONE".to_string(),
        }
    );
    assert_eq!(
        anno.elements[1].1,
        AnnotationValue::Class(expect_desc.clone())
    );
    match &anno.elements[2].1 {
        AnnotationValue::Nested(nested) => {
            assert_eq!(nested.descriptor, expect_desc);
            assert_eq!(
                nested.elements[0].1,
                AnnotationValue::String("app/Helper".to_string()),
                "string values are data, not references"
            );
        }
        other => panic!("nested expected, got {other:?}"),
    }
    assert_eq!(
        anno.elements[3].1,
        AnnotationValue::Array(vec![AnnotationValue::Class(format!("[L{helper};"))])
    );
}
