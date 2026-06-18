# Dashboard Query Workflow
# Queries analytics aggregates and alert history for the dashboard.

import analytics;
import alert;

workflow "dashboard-query" {
    # Step 1: Get temperature aggregate
    let temp_agg = analytics.aggregate({
        "series": "temperature"
    });
    print("Temperature stats: ", temp_agg.points, " points, mean=", temp_agg.mean);

    # Step 2: Get humidity aggregate
    let hum_agg = analytics.aggregate({
        "series": "humidity"
    });
    print("Humidity stats: ", hum_agg.points, " points, mean=", hum_agg.mean);

    # Step 3: Get recent alerts
    let recent_alerts = alert.event_log({
        "limit": 10
    });
    print("Recent alerts: ", recent_alerts.total, " total");

    # Step 4: List alert rules
    let rules = alert.list_rules({});
    print("Alert rules configured: ", rules.rules, " rules");

    # Step 5: Build summary
    let summary = {
        "temperature_avg": temp_agg.mean,
        "humidity_avg": hum_agg.mean,
        "alert_count": recent_alerts.total,
        "status": "healthy"
    };
    print("Dashboard summary: ", summary);
}
