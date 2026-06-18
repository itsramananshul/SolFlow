import brick_store;
import logistics;

workflow "brick-factory" {
    print("=== OpenPrem Brick Factory Automation ===");

    while (true) {
        print("Checking brick stock and factory status...");
        let status = brick_store.status({});
        print("Bricks in stock: " + to_str(status.bricks_in_stock));
        print("Production rate: " + to_str(status.production_rate) + " per cycle");
        print("Revenue: $" + to_str(status.revenue));
        print("Kiln temp: " + to_str(status.kiln_temp) + "°C");

        if (status.pending_backorder_count > 0) {
            print("BACKORDERS: " + to_str(status.pending_backorder_count) + " orders (" + to_str(status.total_bricks_backordered) + " bricks waiting)");
        }

        let threshold: int = 5000;
        if (status.bricks_in_stock < threshold) {
            print("LOW STOCK (" + to_str(status.bricks_in_stock) + " < " + to_str(threshold) + "). Reordering bricks.");
            let order = logistics.ship({
                quantity: 5000,
                destination: "Factory Warehouse",
                priority: "high"
            });
            print("Brick shipment created: " + order.tracking);
            print("ETA: " + to_str(order.eta_days) + " days");
        } else {
            print("Stock level OK (" + to_str(status.bricks_in_stock) + " bricks available)");
        }

        print("Tracking incoming brick shipments...");
        let track = logistics.track({tracking_id: "BRK-BAT-001"});
        print("Shipment: " + track.status + " at " + track.location);
        print("Arrival: " + track.estimated_arrival);

        print("=== Factory cycle complete ===");
        sleep(3000);
    }
}
