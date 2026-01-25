use regex::Regex;
use std::collections::HashSet;

fn main() {
    let script_content = r#"
    // Test state
    state parentCount = 10;
    state showExtra = false;

    // Test data
    // Test data
    state items = [{ id: 1, name: 'Item A', active: true }, { id: 2, name: 'Item B', active: false }, { id: 3, name: 'Item C', active: true }];

    function toggleExtra() {
        showExtra = !showExtra;
    }

    function incrementParent() {
        parentCount += 10;
    }
"#;

    println!("Script content length: {}", script_content.len());

    let mut bindings = HashSet::new();
    let state_decl_re = Regex::new(r"(?:^|[\n;{}])\s*state\s+([a-zA-Z_$][a-zA-Z0-9_$]*)").unwrap();

    for cap in state_decl_re.captures_iter(script_content) {
        println!("Found binding: {}", &cap[1]);
        bindings.insert(cap[1].to_string());
    }

    if bindings.is_empty() {
        println!("No bindings found!");
    } else {
        println!("Bindings found: {:?}", bindings);
    }
}
