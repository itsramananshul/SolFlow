# Sensor Data Ingest Workflow
# Collects sensor readings and stores via gateway + analytics.

import sensor;
import gateway;
import analytics;

workflow "sensor-ingest" {
    # Step 1: Read temperature and humidity sensors
    let temp = sensor.temperature({});
    let humidity = sensor.humidity({});

    print("Temp: ", temp.temperature_c, " deg C");
    print("Humidity: ", humidity.humidity_pct, "%");

    # Step 2: Ingest via gateway
    let ingest_result = gateway.ingest({
        "sensor": temp.sensor,
        "temperature_c": temp.temperature_c,
        "humidity_pct": humidity.humidity_pct
    });
    print("Gateway ingest: ", ingest_result.buffered, " buffered");

    # Step 3: Store temperature in analytics DB
    let store_result = analytics.store({
        "series": "temperature",
        "value": temp.temperature_c,
        "timestamp": temp.timestamp
    });
    print("Analytics stored: ", store_result.stored, " points");

    # Step 4: Also store humidity
    let store_humidity = analytics.store({
        "series": "humidity",
        "value": humidity.humidity_pct,
        "timestamp": humidity.timestamp
    });

    print("Sensor ingest complete for: ", temp.sensor);
}
