// entry point
function start() -> int {
    let p: Person = Person {
        name: "evan",
        age: 19,
    };
    fw(p);  /* call forward */
    return 0;
}

/* Person struct */
struct Person {
    name: str,  // full name
    age: int,   /* age in years */
}

// defined after start
function fw(p: Person) {
    print("declared below where im called");
    print(p.name);
    print(p.age);
}
