//! Annotation rewriting: class descriptors follow the mapping, string
//! values are data and stay put.

use noxis_classfile::{Annotation, AnnotationValue};

use super::super::assign::{RenameMaps, rewrite_descriptor};

/// A list of annotations (visible or invisible) - same rewrite for both.
pub(super) fn rewrite_annotations(annotations: &mut [Annotation], maps: &RenameMaps) {
    for anno in annotations.iter_mut() {
        rewrite_annotation(anno, maps);
    }
}

/// Per-parameter annotation lists (`RuntimeVisibleParameterAnnotations`).
pub(super) fn rewrite_parameter_annotations(params: &mut [Vec<Annotation>], maps: &RenameMaps) {
    for param in params.iter_mut() {
        rewrite_annotations(param, maps);
    }
}

fn rewrite_annotation(anno: &mut Annotation, maps: &RenameMaps) {
    anno.descriptor = rewrite_descriptor(&anno.descriptor, &maps.classes);
    for (_, value) in anno.elements.iter_mut() {
        rewrite_value(value, maps);
    }
}

fn rewrite_value(value: &mut AnnotationValue, maps: &RenameMaps) {
    match value {
        // The `'e'` tag: enum type descriptor moves, the constant name does not.
        AnnotationValue::Enum { descriptor, .. } => {
            *descriptor = rewrite_descriptor(descriptor, &maps.classes);
        }
        // The `'c'` tag: class-info descriptor (`LFoo;` or a primitive `I`).
        AnnotationValue::Class(desc) => {
            *desc = rewrite_descriptor(desc, &maps.classes);
        }
        AnnotationValue::Nested(anno) => rewrite_annotation(anno, maps),
        AnnotationValue::Array(values) => {
            for value in values.iter_mut() {
                rewrite_value(value, maps);
            }
        }
        // Scalars and strings hold no class references.
        _ => {}
    }
}
