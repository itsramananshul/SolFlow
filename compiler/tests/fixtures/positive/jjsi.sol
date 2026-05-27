function is_high(val: int, limit: int) -> bool {
    if val > limit {
        return true;
    }
    return false;
}

struct Person {
    name: str,
    age: int,
}

function print_person(p: Person) {
    print(p.name);
    print(p.age);
}

function start() -> int {
    let p: Person = Person {
        name: "evan",
        age: 19,
    };
    print_person(p);
    return 0;
}
