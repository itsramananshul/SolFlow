import weather_station;
import discord_bot;

workflow "weather-alert" {
    print("=== Weather Alert System ===");

    let r1 = weather_station.read({
        station: "alpha",
        temp_c: 38,
        humidity: 82,
        wind_kph: 25
    });
    print("Station alpha: " + to_str(r1.received));

    let r2 = weather_station.read({
        station: "beta",
        temp_c: 22,
        humidity: 55,
        wind_kph: 10
    });
    print("Station beta:  " + to_str(r2.received));

    let t1: int = r1.received.temp_c;
    let t2: int = r2.received.temp_c;
    let h1: int = r1.received.humidity;
    let h2: int = r2.received.humidity;
    let w1: int = r1.received.wind_kph;
    let w2: int = r2.received.wind_kph;

    let f1: int = (t1 * 9 / 5) + 32;
    let f2: int = (t2 * 9 / 5) + 32;

    print("Alpha: " + to_str(f1) + "F, humidity " + to_str(h1) + "%, wind " + to_str(w1) + "kph");
    print("Beta:  " + to_str(f2) + "F, humidity " + to_str(h2) + "%, wind " + to_str(w2) + "kph");

    if (f1 > 100 || f2 > 100) {
        print("CRITICAL: Extreme temperature detected!");
        let avg: int = (f1 + f2) / 2;
        let report: str = "CRITICAL: Extreme temps! Alpha " + to_str(f1) + "F, Beta " + to_str(f2) + "F, Avg " + to_str(avg) + "F";
        let sent: str = discord_bot.send({
            channel: "#weather",
            text: report,
            alert_level: "critical"
        });
        print("Sent: " + to_str(sent));
    } else {
        if (f1 > 85 || f2 > 85) {
            print("WARNING: High temperature detected!");
            let avg: int = (f1 + f2) / 2;
            let report: str = "WARNING: High temps! Alpha " + to_str(f1) + "F, Beta " + to_str(f2) + "F, Avg " + to_str(avg) + "F";
            let sent: str = discord_bot.send({
                channel: "#weather",
                text: report,
                alert_level: "warning"
            });
            print("Sent: " + to_str(sent));
        } else {
            print("OK: All temperatures normal.");
            if (w1 > 40 || w2 > 40) {
                print("WIND ADVISORY: Strong winds detected!");
                let avg: int = (f1 + f2) / 2;
                let report: str = "Normal temps but WIND ADVISORY. Alpha " + to_str(f1) + "F, Beta " + to_str(f2) + "F, Avg " + to_str(avg) + "F";
                let sent: str = discord_bot.send({
                    channel: "#weather",
                    text: report,
                    alert_level: "wind"
                });
                print("Sent: " + to_str(sent));
            } else {
                let avg: int = (f1 + f2) / 2;
                let report: str = "All clear. Alpha " + to_str(f1) + "F, Beta " + to_str(f2) + "F, Avg " + to_str(avg) + "F";
                let sent: str = discord_bot.send({
                    channel: "#weather",
                    text: report,
                    alert_level: "none"
                });
                print("Sent: " + to_str(sent));
            }
        }
    }

    print("");
    print("=== Done ===");
}
