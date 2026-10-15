//! Per-section struct population, split out of `from_toml::build` purely to
//! stay under the per-file line budget (see `docs/README.md`).

use super::super::sections::{
    ConstantsConfig, DebugConfig, FlowConfig, NameStyle, NoiseConfig, RenameConfig, StringsConfig,
};
use super::super::toml_lite::Table;
use super::{Res, check_known_keys, expect_bool, expect_str, expect_string_vec, expect_usize};

fn parse_name_style(s: &str) -> Res<NameStyle> {
    match s {
        "short-latin" => Ok(NameStyle::ShortLatin),
        "il-o0" => Ok(NameStyle::IlO0),
        "unicode" => Ok(NameStyle::Unicode),
        "mixed-latin" => Ok(NameStyle::MixedLatin),
        "decoy-words" => Ok(NameStyle::DecoyWords),
        "plausible" => Ok(NameStyle::Plausible),
        other => Err(format!("неизвестный стиль имён `{other}`")),
    }
}

pub(super) fn build_rename(t: &Table) -> Res<RenameConfig> {
    check_known_keys(
        t,
        &[
            "spread_packages",
            "package_pool",
            "class_style",
            "member_style",
            "local_style",
            "widen_access",
            "rename_enum_constants",
            "rename_locals",
            "rename_public_members",
        ],
        "rename",
    )?;
    let mut c = RenameConfig::default();
    if let Some(v) = t.get("spread_packages") {
        c.spread_packages = expect_bool(v, "rename.spread_packages")?;
    }
    if let Some(v) = t.get("package_pool") {
        c.package_pool = expect_usize(v, "rename.package_pool")?;
    }
    if let Some(v) = t.get("class_style") {
        c.class_style = parse_name_style(expect_str(v, "rename.class_style")?)?;
    }
    if let Some(v) = t.get("member_style") {
        c.member_style = parse_name_style(expect_str(v, "rename.member_style")?)?;
    }
    if let Some(v) = t.get("local_style") {
        c.local_style = parse_name_style(expect_str(v, "rename.local_style")?)?;
    }
    if let Some(v) = t.get("widen_access") {
        c.widen_access = expect_bool(v, "rename.widen_access")?;
    }
    if let Some(v) = t.get("rename_enum_constants") {
        c.rename_enum_constants = expect_bool(v, "rename.rename_enum_constants")?;
    }
    if let Some(v) = t.get("rename_locals") {
        c.rename_locals = expect_bool(v, "rename.rename_locals")?;
    }
    if let Some(v) = t.get("rename_public_members") {
        c.rename_public_members = expect_bool(v, "rename.rename_public_members")?;
    }
    Ok(c)
}

pub(super) fn build_strings(t: &Table) -> Res<StringsConfig> {
    check_known_keys(
        t,
        &["schemes", "min_len", "split_above", "skip", "derive_key"],
        "strings",
    )?;
    let mut c = StringsConfig::default();
    if let Some(v) = t.get("schemes") {
        c.schemes = expect_string_vec(v, "strings.schemes")?;
    }
    if let Some(v) = t.get("min_len") {
        c.min_len = expect_usize(v, "strings.min_len")?;
    }
    if let Some(v) = t.get("split_above") {
        c.split_above = expect_usize(v, "strings.split_above")?;
    }
    if let Some(v) = t.get("skip") {
        c.skip = expect_string_vec(v, "strings.skip")?;
    }
    if let Some(v) = t.get("derive_key") {
        c.derive_key = expect_bool(v, "strings.derive_key")?;
    }
    Ok(c)
}

pub(super) fn build_constants(t: &Table) -> Res<ConstantsConfig> {
    check_known_keys(t, &["schemes", "max_predicates", "skip"], "constants")?;
    let mut c = ConstantsConfig::default();
    if let Some(v) = t.get("schemes") {
        c.schemes = expect_string_vec(v, "constants.schemes")?;
    }
    if let Some(v) = t.get("max_predicates") {
        c.max_predicates = expect_usize(v, "constants.max_predicates")?;
    }
    if let Some(v) = t.get("skip") {
        c.skip = expect_string_vec(v, "constants.skip")?;
    }
    Ok(c)
}

pub(super) fn build_flow(t: &Table) -> Res<FlowConfig> {
    check_known_keys(t, &["schemes", "max_insns", "encrypt_block_bodies"], "flow")?;
    let mut c = FlowConfig::default();
    if let Some(v) = t.get("schemes") {
        c.schemes = expect_string_vec(v, "flow.schemes")?;
    }
    if let Some(v) = t.get("max_insns") {
        c.max_insns = expect_usize(v, "flow.max_insns")?;
    }
    if let Some(v) = t.get("encrypt_block_bodies") {
        c.encrypt_block_bodies = expect_bool(v, "flow.encrypt_block_bodies")?;
    }
    Ok(c)
}

pub(super) fn build_noise(t: &Table) -> Res<NoiseConfig> {
    check_known_keys(
        t,
        &[
            "junk_ratio",
            "junk_methods_per_class",
            "junk_annotations",
            "weave_reachable",
        ],
        "noise",
    )?;
    let mut c = NoiseConfig::default();
    if let Some(v) = t.get("junk_ratio") {
        c.junk_ratio = expect_usize(v, "noise.junk_ratio")?;
    }
    if let Some(v) = t.get("junk_methods_per_class") {
        c.junk_methods_per_class = expect_usize(v, "noise.junk_methods_per_class")?;
    }
    if let Some(v) = t.get("junk_annotations") {
        c.junk_annotations = expect_usize(v, "noise.junk_annotations")?;
    }
    if let Some(v) = t.get("weave_reachable") {
        c.weave_reachable = expect_bool(v, "noise.weave_reachable")?;
    }
    Ok(c)
}

pub(super) fn build_debug(t: &Table) -> Res<DebugConfig> {
    check_known_keys(
        t,
        &[
            "strip_line_numbers",
            "fake_line_numbers",
            "strip_local_variables",
            "spoof_source_file",
            "strip_source_file",
            "strip_signatures",
        ],
        "debug",
    )?;
    let mut c = DebugConfig::default();
    if let Some(v) = t.get("strip_line_numbers") {
        c.strip_line_numbers = expect_bool(v, "debug.strip_line_numbers")?;
    }
    if let Some(v) = t.get("fake_line_numbers") {
        c.fake_line_numbers = expect_bool(v, "debug.fake_line_numbers")?;
    }
    if let Some(v) = t.get("strip_local_variables") {
        c.strip_local_variables = expect_bool(v, "debug.strip_local_variables")?;
    }
    if let Some(v) = t.get("spoof_source_file") {
        c.spoof_source_file = expect_bool(v, "debug.spoof_source_file")?;
    }
    if let Some(v) = t.get("strip_source_file") {
        c.strip_source_file = expect_bool(v, "debug.strip_source_file")?;
    }
    if let Some(v) = t.get("strip_signatures") {
        c.strip_signatures = expect_bool(v, "debug.strip_signatures")?;
    }
    Ok(c)
}
