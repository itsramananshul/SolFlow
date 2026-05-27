fn main() {
    let src = r#"
struct Point { x: int, y: int }
enum Status { Active, Inactive }
import "io" as io;
function add(a: int, b: int) -> int {
    return a + b;
}
function start() -> int {
    let p: int = 0;
    if (p == 0) {
        print("zero");
    } else {
        print("nonzero");
    }
    while (p < 10) {
        p = p + 1;
    }
    for x in [1, 2, 3] {
        print(x);
    }
    return add(1, 2);
}
"#;
    let result = solflow_compiler::parse_source(src);
    println!("{}", serde_json::to_string_pretty(&result.value).unwrap());
}
