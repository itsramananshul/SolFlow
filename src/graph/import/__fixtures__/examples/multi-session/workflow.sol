import numbers;
import printeruno;
import printerdos;

workflow "one" {
    while (true) {
        let n = numbers.get();
        printeruno.print(n);
    }
}

workflow "two" {
    while (true) {
        let n = numbers.get();
        printerdos.print(n);
    }
}
