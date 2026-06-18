workflow "big-italy" {
    print("=== Big Italy Workflow ===");

    # Phase 1: Produce ingredients
    let t = call("produce_tomato", {});
    let b = call("produce_bread", {});
    let c = call("produce_cheese", {});
    let p = call("produce_pasta", {});

    print("Produced: tomato=", t.count, " bread=", b.count, " cheese=", c.count, " pasta=", p.count);

    # Phase 2: Deliver ingredients to pizza factory
    call("deliver_pizza", {inventory: {tomato: t.count, bread: b.count, cheese: c.count}});

    # Phase 3: Deliver ingredients to spaghetti factory
    call("deliver_spaghetti", {inventory: {tomato: t.count, pasta: p.count}});

    # Phase 4: Make products
    let pizza = call("make_pizza", {count: 10});
    let spaghetti = call("make_spaghetti", {count: 10});

    print("Made: pizza=", pizza.made, " spaghetti=", spaghetti.made);

    # Phase 5: Check status
    call("status_pizza", {});
    call("status_spaghetti", {});

    print("=== Big Italy Workflow Complete ===");
}
