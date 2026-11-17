// Welcome to the jolt playground!
// This Java is compiled to real JVM bytecode right here in your browser,
// then run by a tiny JVM, both written in Rust and compiled to WebAssembly.
// Edit anything: errors appear as you type. Press Run (Ctrl/Cmd + Enter).

interface Figure {
    double area();
    String name();
}

class Disk implements Figure {
    private final double r;
    Disk(double r) { this.r = r; }
    public double area() { return Math.PI * r * r; }
    public String name() { return "disk"; }
}

class Tile implements Figure {
    private final double side;
    Tile(double side) { this.side = side; }
    public double area() { return side * side; }
    public String name() { return "tile"; }
}

public class Welcome {
    static boolean isPrime(int n) {
        if (n < 2) return false;
        for (int d = 2; d * d <= n; d++) {
            if (n % d == 0) return false;
        }
        return true;
    }

    public static void main(String[] args) {
        System.out.println("Hello from jolt! ⚡");
        System.out.println();

        StringBuilder fizz = new StringBuilder();
        for (int i = 1; i <= 15; i++) {
            fizz.append(i % 15 == 0 ? "FizzBuzz" : i % 3 == 0 ? "Fizz" : i % 5 == 0 ? "Buzz" : String.valueOf(i));
            if (i < 15) fizz.append(", ");
        }
        System.out.println("FizzBuzz: " + fizz);

        System.out.print("First 10 primes:");
        int found = 0;
        for (int n = 2; found < 10; n++) {
            if (isPrime(n)) {
                System.out.print(" " + n);
                found++;
            }
        }
        System.out.println();

        Figure[] figures = { new Disk(1.5), new Tile(2), new Disk(0.5) };
        double total = 0;
        for (Figure f : figures) {
            System.out.printf("  %-5s area = %6.3f%n", f.name(), f.area());
            total += f.area();
        }
        System.out.printf("Total area: %.2f%n", total);

        long fact = 1;
        for (int i = 2; i <= 20; i++) fact *= i;
        System.out.println("20! = " + fact);
    }
}
