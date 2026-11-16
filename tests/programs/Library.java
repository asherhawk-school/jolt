import java.util.Random;
import java.util.Scanner;

public class Library {
    public static void main(String[] args) throws Exception {
        Scanner sc = new Scanner("3 4.5 hello\nsecond line\n42");
        int a = sc.nextInt();
        double b = sc.nextDouble();
        String w = sc.next();
        sc.nextLine();
        String line = sc.nextLine();
        System.out.println(a + " " + b + " " + w + " [" + line + "] " + sc.hasNextInt() + " " + sc.nextInt() + " " + sc.hasNext());
        sc.close();

        Random r = new Random(42);
        int sum = 0;
        for (int i = 0; i < 100; i++) sum += r.nextInt(10);
        System.out.println(sum + " " + r.nextBoolean() + " " + r.nextLong());
        System.out.printf("%.6f%n", r.nextDouble());

        long t0 = System.nanoTime();
        Thread.sleep(5);
        System.out.println((System.nanoTime() - t0) >= 5_000_000L);

        int[] src = {1, 2, 3, 4, 5};
        int[] dst = new int[7];
        System.arraycopy(src, 1, dst, 2, 3);
        System.out.println(java.util.Arrays.toString(dst));
        System.out.println(Math.hypot(3, 4) + " " + Math.cbrt(27) + " " + Math.signum(-2.0) + " " + Math.toDegrees(Math.PI));
        System.out.println(Integer.bitCount(255) + " " + Long.numberOfTrailingZeros(64L) + " " + Integer.compare(3, 9) + " " + Character.isLetterOrDigit('_'));
        System.out.println(Double.compare(0.0, -0.0) + " " + Double.isNaN(0.0 / 0) + " " + Double.MAX_VALUE + " " + Double.MIN_VALUE);
        System.out.println(Boolean.parseBoolean("TRUE") + " " + Integer.parseInt("ff", 16) + " " + Character.forDigit(11, 16) + Character.digit('7', 10));
        Object o = 7;
        System.out.println(o + " " + (o instanceof Integer));
        System.err.println("to stderr");
    }
}
