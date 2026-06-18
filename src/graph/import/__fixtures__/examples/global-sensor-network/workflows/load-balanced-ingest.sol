# Cross-Region Load-Balanced Ingest
# Demonstrates RAIN load-balanced proxy across multiple instances.

import sensor;

workflow "load-balanced-ingest" {
    # Step 1-5: Read temperature from each sensor controller
    # RAIN Load Balancer distributes across sensor instances
    let reading1 = sensor.temperature({});
    let reading2 = sensor.temperature({});
    let reading3 = sensor.temperature({});
    let reading4 = sensor.temperature({});
    let reading5 = sensor.temperature({});

    # Step 6: Collect all readings
    let all_temps = [
        reading1.temperature_c,
        reading2.temperature_c,
        reading3.temperature_c,
        reading4.temperature_c,
        reading5.temperature_c
    ];

    print("Readings collected: ", all_temps.len(), " samples");

    # Step 7: Compute min, max, avg
    let min_temp = 100;
    let max_temp = -100;
    let sum_temp = 0;

    for t in all_temps {
        if t < min_temp { min_temp = t; }
        if t > max_temp { max_temp = t; }
        sum_temp = sum_temp + t;
    }

    let avg_temp = sum_temp / all_temps.len();
    print("Min: ", min_temp, " Max: ", max_temp, " Avg: ", avg_temp);

    # Step 8: Detect anomalies
    if max_temp - min_temp > 10 {
        print("ANOMALY: Large temperature spread detected across regions!");
    } else {
        print("OK: Temperature spread within normal range.");
    }
}
