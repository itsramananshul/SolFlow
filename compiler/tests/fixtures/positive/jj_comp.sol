struct SystemNode {
    id: int,
    threshold: float
}

function assess_node(node: SystemNode, limit: float) -> bool {
    print("Checking Node:");
    print(node.id);
    print("Value:");
    print(node.threshold);

    if node.threshold > limit {
        print("ALERT: Node exceeded limit!");
        return true;
    }
    
    print("Node status: Nominal");
    return false;
}

function start() -> int {
    let limit_val: float = 85.0;
    let counter: int = 1;

    while counter < 4 {
        print("--- Cycle ---");
        print(counter);

        let current_reading: float = 78.5 + (counter * 3.0);

        let node_instance: SystemNode = SystemNode {
            id: counter,
            threshold: current_reading,
        };

        let is_dangerous: bool = assess_node(node_instance, limit_val);
        
        counter = counter + 1;
    }

    print("Execution finished successfully.");
    return 0;
}
