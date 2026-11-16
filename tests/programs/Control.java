public class Control {
    static String classify(int n) {
        if (n < 0) return "negative";
        else if (n == 0) return "zero";
        else if (n % 2 == 0) return "even";
        return "odd";
    }

    static int sumTo(int n) {
        int s = 0;
        for (int i = 1; i <= n; i++) s += i;
        return s;
    }

    static int findFirst(int[] a, int v) {
        for (int i = 0; i < a.length; i++) {
            if (a[i] == v) return i;
        }
        return -1;
    }

    static int loopForever() {
        int i = 0;
        while (true) {
            i++;
            if (i * i > 50) return i;
        }
    }

    static int doLoop(int n) {
        int count = 0;
        do {
            count++;
            n /= 2;
        } while (n > 0);
        return count;
    }

    public static void main(String[] args) {
        for (int i = -1; i <= 3; i++) System.out.println(i + " " + classify(i));
        System.out.println(sumTo(100));
        System.out.println(findFirst(new int[]{4, 8, 15, 16, 23, 42}, 16));
        System.out.println(findFirst(new int[]{4, 8}, 3));
        System.out.println(loopForever());
        System.out.println(doLoop(1000));

        outer:
        for (int i = 0; i < 5; i++) {
            for (int j = 0; j < 5; j++) {
                if (j == 3) continue outer;
                if (i == 3) break outer;
                System.out.print(i * 10 + j + " ");
            }
        }
        System.out.println();

        int k = 0;
        while (k < 20) {
            k++;
            if (k % 3 == 0) continue;
            if (k > 10) break;
            System.out.print(k + ",");
        }
        System.out.println();

        int n = 27, steps = 0;
        while (n != 1) {
            n = n % 2 == 0 ? n / 2 : 3 * n + 1;
            steps++;
        }
        System.out.println("collatz " + steps);

        for (int i = 0, j = 10; i < j; i += 2, j -= 3) System.out.print("(" + i + "," + j + ")");
        System.out.println();

        int x = 5;
        String s = x > 3 ? (x > 4 ? "big" : "medium") : "small";
        System.out.println(s);
        char g = x > 3 ? 'Y' : 'N';
        System.out.println(g);
        long lv = x > 3 ? 1 : 2L;
        System.out.println(lv);
        System.out.println(x > 3 ? 1 : 2.5);

        boolean found = false;
        int idx = 0;
        while (!found && idx < 10) {
            if (idx * idx == 49) found = true;
            else idx++;
        }
        System.out.println(found + " " + idx);

        int total = 0;
        for (int i = 0; i < 100; i++) {
            if (i % 7 == 0 || i % 11 == 0) {
                if (i % 2 == 0 && i > 10) continue;
                total += i;
            }
        }
        System.out.println(total);

        block: {
            if (total > 0) break block;
            System.out.println("not printed");
        }
        System.out.println("after block");

        int depth = 0;
        for (;;) {
            depth++;
            if (depth >= 4) break;
        }
        System.out.println(depth);

        var list = new int[]{3, 1, 2};
        var sum = 0;
        for (var v : list) sum += v;
        System.out.println(sum);
        for (long v : list) System.out.print(v * 1000000000L + " ");
        System.out.println();
        String[] words = {"alpha", "beta", "gamma"};
        for (String w : words) {
            if (w.startsWith("b")) continue;
            System.out.println(w.toUpperCase());
        }
    }
}
