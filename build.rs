use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");

    let manifest = fs::read_to_string("Cargo.toml").unwrap();

    let current_chapter: usize = manifest
        .lines()
        .find_map(|line| {
            let path = line.trim().strip_prefix("path = \"src/chapter")?;
            let number = path.strip_suffix(".rs\"")?;
            number.parse().ok()
        })
        .expect("No chapter path found in Cargo.toml");

    println!("Current chapter: {current_chapter}");

    for n in 1..=30 {
        println!("cargo:rustc-check-cfg=cfg(only{n:02})");
        println!("cargo:rustc-check-cfg=cfg(until{n:02})");
        println!("cargo:rustc-check-cfg=cfg(since{n:02})");
        if current_chapter == n { println!("cargo:rustc-cfg=only{n:02}"); }
        if current_chapter <= n { println!("cargo:rustc-cfg=until{n:02}"); }
        if current_chapter >= n { println!("cargo:rustc-cfg=since{n:02}"); }
    }
}
