//! Одноразовый отладочный обход байткода (scratch): ручной парсинг пула и
//! Code-атрибутов, вывод заголовков вложенных атрибутов.
#![allow(clippy::unwrap_used, clippy::expect_used)]

fn rd_u16(b: &[u8], p: &mut usize) -> u16 {
    let v = u16::from_be_bytes([b[*p], b[*p + 1]]);
    *p += 2;
    v
}
fn rd_u32(b: &[u8], p: &mut usize) -> u32 {
    let v = u32::from_be_bytes([b[*p], b[*p + 1], b[*p + 2], b[*p + 3]]);
    *p += 4;
    v
}
fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
fn main() {
    let entries = noxis_jar::read_file(std::path::Path::new(
        "/storage/project/jvm/LoReport/build/libs/lorep-1.1.0.jar",
    ))
    .unwrap();
    let program = noxis_jar::load(entries).unwrap();
    let class = program
        .classes
        .iter()
        .find(|c| c.this_class.ends_with("ReportStatsCommand"))
        .unwrap();
    let prog = noxis_classfile::Program {
        classes: vec![class.clone()],
        resources: vec![],
    };
    let out = noxis_jar::save(&prog).unwrap();
    let entry = out.iter().find(|e| e.name.ends_with(".class")).unwrap();
    let bytes = &entry.data;
    // find method_execute Code attr -> sub attrs, dump each sub-attr name_idx + len + first bytes
    // manual walk: pool
    let mut pos = 8;
    let pool_count = rd_u16(bytes, &mut pos);
    // collect utf8 entries for name resolution
    let mut utf8s: std::collections::BTreeMap<u16, String> = Default::default();
    let mut idx = 1u16;
    while idx < pool_count {
        let tag = bytes[pos];
        pos += 1;
        match tag {
            1 => {
                let len = rd_u16(bytes, &mut pos) as usize;
                utf8s.insert(
                    idx,
                    String::from_utf8_lossy(&bytes[pos..pos + len]).into_owned(),
                );
                pos += len;
            }
            3 | 4 => pos += 4,
            5 | 6 => {
                pos += 8;
                idx += 1;
            }
            7 | 8 | 16 | 19 | 20 => pos += 2,
            9 | 10 | 11 | 12 | 17 | 18 => pos += 4,
            15 => pos += 3,
            _ => {
                eprintln!("pool tag {tag}");
                return;
            }
        }
        idx += 1;
    }
    let name_of = |i: u16| utf8s.get(&i).cloned().unwrap_or(format!("#{i}"));
    pos += 6;
    let ifaces = rd_u16(bytes, &mut pos);
    pos += 2 * ifaces as usize;
    let fields = rd_u16(bytes, &mut pos);
    for _ in 0..fields {
        pos += 6;
        let n = rd_u16(bytes, &mut pos);
        for _ in 0..n {
            pos += 2;
            let l = rd_u32(bytes, &mut pos);
            pos += l as usize;
        }
    }
    let methods = rd_u16(bytes, &mut pos);
    for _ in 0..methods {
        pos += 2;
        let ni = rd_u16(bytes, &mut pos);
        pos += 2;
        let an = rd_u16(bytes, &mut pos);
        let mname = name_of(ni);
        for _ in 0..an {
            let aname = rd_u16(bytes, &mut pos);
            let alen = rd_u32(bytes, &mut pos) as usize;
            let aname_s = name_of(aname);
            if aname_s == "Code" {
                let start = pos;
                pos += 4;
                let clen = rd_u32(bytes, &mut pos) as usize;
                pos += clen;
                let exc = rd_u16(bytes, &mut pos) as usize;
                pos += 8 * exc;
                let subn = rd_u16(bytes, &mut pos);
                for _ in 0..subn {
                    let sname = rd_u16(bytes, &mut pos);
                    let slen = rd_u32(bytes, &mut pos) as usize;
                    eprintln!(
                        "{mname}: sub={} len={slen} head=[{}]",
                        name_of(sname),
                        hex(&bytes[pos..pos + slen.min(24)])
                    );
                    pos += slen;
                }
                assert_eq!(pos, start + alen, "Code length mismatch in {mname}");
            } else {
                pos += alen;
            }
        }
    }
    eprintln!("walk ok");
}
