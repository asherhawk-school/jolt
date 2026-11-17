import java.util.Scanner;

public class Input {
    public static void main(String[] args) {
        Scanner in = new Scanner(System.in);
        int n = in.nextInt();
        long sum = 0;
        int max = Integer.MIN_VALUE;
        for (int i = 0; i < n; i++) {
            int v = in.nextInt();
            sum += v;
            max = Math.max(max, v);
        }
        in.nextLine();
        String name = in.nextLine();
        System.out.printf("n=%d sum=%d max=%d avg=%.2f%n", n, sum, max, (double) sum / n);
        System.out.println("Hello, " + name.trim() + "!");
        while (in.hasNext()) {
            if (in.hasNextInt()) System.out.println("int: " + in.nextInt());
            else if (in.hasNextDouble()) System.out.println("double: " + in.nextDouble());
            else System.out.println("word: " + in.next());
        }
    }
}
