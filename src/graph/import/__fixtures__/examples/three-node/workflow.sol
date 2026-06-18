import app_b1;
import app_b2;
import app_c1;

workflow "demo" {
    while (true) {
        print("=== Three-Node Demo ===");
        print("");

        print("Step 1: Reading temperature from B1...");
        let temp = app_b1.get_temp({ sensor: "rooftop" });
        print("Temperature: " + to_str(temp.celsius) + "C / " + to_str(temp.fahrenheit) + "F");
        print("Sensor: " + temp.sensor);
        print("");

        print("Step 2: Logging result to B2...");
        let log_res = app_b2.log({ text: "Temp check: " + to_str(temp.celsius) + "C" });
        print("Logged: " + to_str(log_res.logged));
        print("");

        print("Step 3: Sending alert from C1...");
        let alert = app_c1.notify({ message: "Temperature reading complete: " + to_str(temp.celsius) + "C" });
        print("Alert sent: " + to_str(alert.notified));
        print("");

        print("=== Demo Complete ===");
    }
}
