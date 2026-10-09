//! Descriptor parsing (`(...)V`, `[L...;`).

use super::super::VerificationType;

/// Descriptor parse error (context is added by the caller).
#[derive(Debug)]
pub(crate) struct DescError(pub String);

/// Write value slots into `locals` at position; return next position.
pub(crate) fn write_local(
    locals: &mut [VerificationType],
    slot: usize,
    value: Vec<VerificationType>,
) -> Result<usize, ()> {
    if slot + value.len() > locals.len() {
        return Err(());
    }
    let width = value.len().max(1);
    for (offset, item) in value.into_iter().enumerate() {
        locals[slot + offset] = item;
    }
    Ok(slot + width)
}

/// Parse an array descriptor into (depth, element without `;`).
pub(crate) fn split_array(descriptor: &str) -> Option<(usize, &str)> {
    let dims = descriptor.bytes().take_while(|&b| b == b'[').count();
    if dims == 0 {
        return None;
    }
    Some((dims, descriptor[dims..].trim_end_matches(';')))
}

/// Slots for a single field descriptor (`long`/`double` — pair with `Top`).
pub(crate) fn field_slots(descriptor: &str) -> Result<Vec<VerificationType>, DescError> {
    let mut chars = descriptor.chars();
    match chars.next() {
        Some('B') | Some('C') | Some('I') | Some('S') | Some('Z') => {
            Ok(vec![VerificationType::Integer])
        }
        Some('F') => Ok(vec![VerificationType::Float]),
        Some('J') => Ok(vec![VerificationType::Long, VerificationType::Top]),
        Some('D') => Ok(vec![VerificationType::Double, VerificationType::Top]),
        Some('V') => Ok(Vec::new()),
        Some('L') => {
            let mut name = String::new();
            for char in chars {
                if char == ';' {
                    return Ok(vec![VerificationType::Object(name)]);
                }
                name.push(char);
            }
            Err(DescError(format!("нет `;` в {descriptor}")))
        }
        Some('[') => Ok(vec![VerificationType::Object(descriptor.to_string())]),
        _ => Err(DescError(format!("кривой дескриптор {descriptor}"))),
    }
}

/// Parameter and return slots (`None` — `void`).
type MethodSlots = (Vec<Vec<VerificationType>>, Option<Vec<VerificationType>>);

/// Method descriptor: parameter slots and return slots (`None` — `void`).
pub(crate) fn method_slots(descriptor: &str) -> Result<MethodSlots, DescError> {
    let (params, ret) = descriptor
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .ok_or_else(|| DescError(format!("кривой дескриптор {descriptor}")))?;
    let mut out = Vec::new();
    let mut index = 0;
    let bytes = params.as_bytes();
    while index < bytes.len() {
        match bytes[index] as char {
            'B' | 'C' | 'I' | 'S' | 'Z' => {
                out.push(vec![VerificationType::Integer]);
                index += 1;
            }
            'F' => {
                out.push(vec![VerificationType::Float]);
                index += 1;
            }
            'J' => {
                out.push(vec![VerificationType::Long, VerificationType::Top]);
                index += 1;
            }
            'D' => {
                out.push(vec![VerificationType::Double, VerificationType::Top]);
                index += 1;
            }
            'L' => {
                let end = params[index..]
                    .find(';')
                    .ok_or_else(|| DescError(format!("нет `;` в {descriptor}")))?;
                out.push(vec![VerificationType::Object(
                    params[index + 1..index + end].to_string(),
                )]);
                index += end + 1;
            }
            '[' => {
                let mut end = index;
                while bytes[end] == b'[' {
                    end += 1;
                }
                if bytes[end] == b'L' {
                    let semi = params[end..]
                        .find(';')
                        .ok_or_else(|| DescError(format!("нет `;` в {descriptor}")))?;
                    end += semi + 1;
                } else {
                    end += 1;
                }
                if end > bytes.len() {
                    return Err(DescError(format!("кривой дескриптор {descriptor}")));
                }
                out.push(vec![VerificationType::Object(
                    params[index..end].to_string(),
                )]);
                index = end;
            }
            _ => return Err(DescError(format!("кривой дескриптор {descriptor}"))),
        }
    }
    let ret = if ret == "V" {
        None
    } else {
        Some(field_slots(ret)?)
    };
    Ok((out, ret))
}

/// Element type from an array descriptor on the stack.
pub(crate) fn array_element(
    array: &VerificationType,
    kind: crate::insn::ArrayKind,
) -> VerificationType {
    use crate::insn::ArrayKind::{Boolean, Byte, Char, Double, Float, Int, Long, Reference, Short};
    if let VerificationType::Object(descriptor) = array
        && let Some((dims, elem)) = split_array(descriptor)
    {
        if dims > 1 {
            return VerificationType::Object(descriptor[1..].to_string());
        }
        return match elem {
            "Z" | "B" | "C" | "S" | "I" => VerificationType::Integer,
            "J" => VerificationType::Long,
            "F" => VerificationType::Float,
            "D" => VerificationType::Double,
            _ => VerificationType::Object(format!("[{elem};")),
        };
    }
    match kind {
        Boolean | Byte | Short | Char | Int => VerificationType::Integer,
        Long => VerificationType::Long,
        Float => VerificationType::Float,
        Double => VerificationType::Double,
        Reference => VerificationType::Object("java/lang/Object".to_string()),
    }
}
