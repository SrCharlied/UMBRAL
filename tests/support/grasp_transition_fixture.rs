// Synthetic growth generator parameterized ONLY by ring count.
// Derived from unchanged spatial_pursuit_fixture.rs growth_text.
fn growth_text_with_rings(capture: &str, n: usize) -> String {
    assert!(n >= 2);
    let ring_index = |j, k| {
        if j == 0 {
            pidx(CUT, k)
        } else if j == n {
            didx(CUT, k)
        } else {
            gidx(j, k)
        }
    };
    let a = tube(1);
    let mut l = vec![
        "UMB311_GROWTH_PACKAGE 1".to_string(),
        format!(
            "capture_package_fnv1a64 {:016x}",
            fnv1a64(capture.as_bytes())
        ),
        format!("topology_sha256 {}", "a".repeat(64)),
        format!("law_sha256 {}", "b".repeat(64)),
        format!("approval_sha256 {}", "c".repeat(64)),
        format!("frames {FRAMES}"),
    ];
    let nv = (CUT + 1) * SEGS + (RINGS - CUT) * SEGS + (n - 1) * SEGS;
    l.push(format!(
        "counts {nv} {} {SEGS} {n}",
        a.faces.len() + 2 * SEGS * n
    ));
    for r in 0..=CUT {
        for s in 0..SEGS {
            let kind = if r == CUT { "orig_on_cut" } else { "orig" };
            l.push(format!("vertex {kind} P {}", r * SEGS + s));
        }
    }
    for r in CUT..RINGS {
        for s in 0..SEGS {
            let kind = if r == CUT { "orig_on_cut" } else { "orig" };
            l.push(format!("vertex {kind} D {}", r * SEGS + s));
        }
    }
    for j in 1..n {
        for k in 0..SEGS {
            l.push(format!("vertex ring {j} {k}"));
        }
    }
    let ids = |v: Vec<usize>| {
        v.iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    l.push(format!(
        "loop_p {}",
        ids((0..SEGS).map(|k| pidx(CUT, k)).collect())
    ));
    l.push(format!(
        "loop_d {}",
        ids((0..SEGS).map(|k| didx(CUT, k)).collect())
    ));
    for j in 0..=n {
        l.push(format!(
            "ring {j} {}",
            ids((0..SEGS).map(|k| ring_index(j, k)).collect())
        ));
    }
    for (fi, f) in a.faces.iter().enumerate() {
        let v = f.vertex_indices;
        let r = v.iter().map(|i| i / SEGS).min().unwrap();
        let (part, map): (&str, fn(usize, usize) -> usize) =
            if r < CUT { ("P", pidx) } else { ("D", didx) };
        let tv: Vec<usize> = v.iter().map(|&i| map(i / SEGS, i % SEGS)).collect();
        let uv: Vec<String> = v
            .iter()
            .flat_map(|&i| a.uvs[i])
            .map(|x| format!("{x:?}"))
            .collect();
        l.push(format!(
            "face {part} {} {} {} {} c:{fi}:0 c:{fi}:1 c:{fi}:2",
            tv[0],
            tv[1],
            tv[2],
            uv.join(" ")
        ));
    }
    for j in 0..n {
        for k in 0..SEGS {
            let k1 = (k + 1) % SEGS;
            for tri in [
                [(k1, j), (k, j), (k, j + 1)],
                [(k1, j), (k, j + 1), (k1, j + 1)],
            ] {
                let v: Vec<String> = tri
                    .iter()
                    .map(|&(k, j)| ring_index(j, k).to_string())
                    .collect();
                let uv: Vec<String> = tri
                    .iter()
                    .flat_map(|&(k, j)| [k as f64 / SEGS as f64, j as f64 / n as f64])
                    .map(|x| format!("{x:?}"))
                    .collect();
                let src: Vec<String> = tri.iter().map(|&(k, j)| format!("g:{j}:{k}")).collect();
                l.push(format!(
                    "face G {} {} {}",
                    v.join(" "),
                    uv.join(" "),
                    src.join(" ")
                ));
            }
        }
    }
    let mask = |name: &str, v: Vec<usize>| format!("mask {name} {} {}", v.len(), ids(v));
    l.push(mask("root", (0..SEGS).map(|s| pidx(0, s)).collect()));
    l.push(mask(
        "grip",
        (0..SEGS).map(|s| didx(RINGS - 1, s)).collect(),
    ));
    l.push(mask(
        "coil",
        (0..SEGS).map(|s| didx(RINGS - 2, s)).collect(),
    ));
    for f in 1..=FRAMES {
        l.push(format!("guide {f} 1.0 0.0 0.0"));
    }
    l.push("end".into());
    l.join("\n") + "\n"
}
