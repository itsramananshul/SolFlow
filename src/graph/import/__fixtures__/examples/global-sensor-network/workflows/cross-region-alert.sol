# Cross-Region Alert Workflow
# Reads sensors from all 3 regions and evaluates alert rules.

import sensor;
import alert;

workflow "cross-region-alert" {
    # Step 1: Read temperature from US-East (DC1)
    let temp_east = sensor.temperature({});
    print("US-East DC1 temp: ", temp_east.temperature_c);

    # Step 2: Read temperature from US-West (DC3)
    let temp_west = sensor.temperature({});
    print("US-West DC3 temp: ", temp_west.temperature_c);

    # Step 3: Read temperature from EU-West (DC5)
    let temp_eu = sensor.temperature({});
    print("EU-West DC5 temp: ", temp_eu.temperature_c);

    # Step 4: Evaluate alerts on the east reading
    let alert_result = alert.evaluate({
        "temperature_c": temp_east.temperature_c,
        "sensor": "US-East/DC1"
    });
    print("Alert check: ", alert_result.alerts_triggered, " alerts");

    # Step 5: Compute cross-region average
    let avg_temp = (temp_east.temperature_c + temp_west.temperature_c + temp_eu.temperature_c) / 3;
    print("Cross-region average temp: ", avg_temp);

    # Step 6: Flag if any region is in alert
    if (alert_result.alerts_triggered > 0) {
        print("WARNING: Alerts triggered in US-East!");
    } else {
        print("OK: No alerts in US-East");
    }

    print("Cross-region alert check complete.");
}
