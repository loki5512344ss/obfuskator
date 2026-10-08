//! Структура «программы» — единица работы пайплайна.

pub mod builder;
pub mod version;

pub use version::Version;

use crate::format::StackMapFrame;
use crate::insn::{Const, Insn, Label};

/// Неизвестные/специфичные атрибуты хранятся байтами — round-trip без потерь.
#[derive(Debug, Clone, PartialEq)]
pub enum Attribute {
    SourceFile(String),
    LineNumberTable(Vec<LineNumber>),
    LocalVariableTable(Vec<LocalVariable>),
    Signature(String),
    Exceptions(Vec<String>),
    Deprecated,
    Synthetic,
    InnerClasses(Vec<InnerClass>),
    /// `ConstantValue` поля (`static final`): значение из пула.
    ConstantValue(Const),
    /// `BootstrapMethods` для `invokedynamic` (лямбды): метод + аргументы.
    BootstrapMethods(Vec<BootstrapMethod>),
    /// `EnclosingMethod`: класс и опциональный метод, содержащие локальный класс.
    EnclosingMethod {
        class: String,
        method: Option<(String, String)>,
    },
    /// `MethodParameters`: имена формальных параметров (`None` — безымянный).
    MethodParameters(Vec<MethodParameter>),
    /// `Record`: компоненты record-класса.
    Record(Vec<RecordComponent>),
    /// `LocalVariableTypeTable`: сигнатуры generic-локальных (как LVT).
    LocalVariableTypeTable(Vec<LocalVariableType>),
    /// Всё остальное: имя + сырые байты `info` (осторожно с индексами пула).
    Raw {
        name: String,
        data: Vec<u8>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineNumber {
    /// Метка начала (байты вычисляет writer — переживает вставки пассов).
    pub start: Label,
    pub line: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalVariable {
    /// Метки диапазона видимости (writer пересчитает в байты).
    pub start: Label,
    pub end: Label,
    pub name: String,
    pub descriptor: String,
    pub index: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InnerClass {
    pub inner: String,
    pub outer: Option<String>,
    pub inner_name: Option<String>,
    pub access_flags: u16,
}

/// Один `bootstrap`-метод: ссылка вида `MethodHandle` + аргументы-константы.
#[derive(Debug, Clone, PartialEq)]
pub struct BootstrapMethod {
    /// Номер `reference_kind` (JVMS §5.4.3.5).
    pub kind: u8,
    pub owner: String,
    pub name: String,
    pub descriptor: String,
    pub itf: bool,
    pub args: Vec<Const>,
}

/// Формальный параметр метода (`MethodParameters`).
#[derive(Debug, Clone, PartialEq)]
pub struct MethodParameter {
    pub name: Option<String>,
    pub access_flags: u16,
}

/// Компонент `Record`: поле record-класса с атрибутами.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordComponent {
    pub name: String,
    pub descriptor: String,
    pub attributes: Vec<Attribute>,
}

/// Строка `LocalVariableTypeTable` (как `LocalVariable`, но с сигнатурой).
#[derive(Debug, Clone, PartialEq)]
pub struct LocalVariableType {
    /// Метки диапазона видимости (writer пересчитает в байты).
    pub start: Label,
    pub end: Label,
    pub name: String,
    pub signature: String,
    pub index: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub access_flags: u16,
    pub name: String,
    pub descriptor: String,
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub access_flags: u16,
    pub name: String,
    pub descriptor: String,
    pub code: Option<Code>,
    pub attributes: Vec<Attribute>,
}

impl Method {
    /// `native`/`abstract` тела не имеют.
    pub fn has_code(&self) -> bool {
        self.code.is_some()
    }
}

/// Тело метода.
#[derive(Debug, Clone, PartialEq)]
pub struct Code {
    pub max_stack: u16,
    pub max_locals: u16,
    /// Маркеры меток + инструкции; ветвления указывают на [`Label`].
    pub insns: Vec<Insn>,
    pub exception_table: Vec<ExceptionHandler>,
    pub frames: Vec<StackMapFrame>,
    pub attributes: Vec<Attribute>,
}

impl Code {
    /// Все метки, объявленные в теле метода.
    pub fn labels(&self) -> Vec<Label> {
        self.insns
            .iter()
            .filter_map(|i| match i {
                Insn::Mark(l) => Some(*l),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExceptionHandler {
    /// `None` — catch-all (`finally`).
    pub start: Label,
    pub end: Label,
    pub handler: Label,
    /// Внутреннее имя класса или `None` для `any`.
    pub catch_type: Option<String>,
}

/// Один `.class` в нормализованном виде.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassFile {
    pub version: Version,
    pub access_flags: u16,
    pub this_class: String,
    pub super_class: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<Field>,
    pub methods: Vec<Method>,
    pub attributes: Vec<Attribute>,
}

impl ClassFile {
    /// Ищет метод по имени и дескриптору.
    pub fn method(&self, name: &str, descriptor: &str) -> Option<&Method> {
        self.methods
            .iter()
            .find(|m| m.name == name && m.descriptor == descriptor)
    }

    /// Ищет метод по имени и дескриптору (мутирующий доступ).
    pub fn method_mut(&mut self, name: &str, descriptor: &str) -> Option<&mut Method> {
        self.methods
            .iter_mut()
            .find(|m| m.name == name && m.descriptor == descriptor)
    }

    /// Признак `ACC_INTERFACE`.
    pub fn is_interface(&self) -> bool {
        self.access_flags & 0x0200 != 0
    }
}

/// Ресурс внутри архива (JSON, properties, картинки, ...).
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub path: String,
    pub data: Vec<u8>,
}

/// Вся входная программа: классы + ресурсы.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Program {
    pub classes: Vec<ClassFile>,
    pub resources: Vec<Resource>,
}

impl Program {
    /// Класс по внутреннему имени (`pkg/Class`).
    pub fn class(&self, internal_name: &str) -> Option<&ClassFile> {
        self.classes.iter().find(|c| c.this_class == internal_name)
    }

    /// Класс по внутреннему имени (мутирующий доступ).
    pub fn class_mut(&mut self, internal_name: &str) -> Option<&mut ClassFile> {
        self.classes
            .iter_mut()
            .find(|c| c.this_class == internal_name)
    }

    /// Ресурс по пути внутри архива.
    pub fn resource(&self, path: &str) -> Option<&Resource> {
        self.resources.iter().find(|r| r.path == path)
    }

    /// Пустая ли программа.
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.resources.is_empty()
    }
}
