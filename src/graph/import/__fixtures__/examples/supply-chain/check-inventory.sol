workflow "check-inventory" {
    let inv: int = call("central-warehouse.inventory", {});
    print("Central warehouse stock:");
    print(inv);
    if (inv < 100) {
        print("Low stock! Purchasing more...");
        let result: str = call("central-warehouse.purchase", {shop: "Corner-Shop", brick_type: "red", count: 50});
        print(result);
    } else {
        print("Stock is sufficient.");
    }
    print("Workflow complete.");
}
