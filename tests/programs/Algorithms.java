import java.util.Arrays;

public class Algorithms {
    static long[] memo = new long[100];

    static long fib(int n) {
        if (n < 2) return n;
        if (memo[n] != 0) return memo[n];
        return memo[n] = fib(n - 1) + fib(n - 2);
    }

    static int fibSlow(int n) { return n < 2 ? n : fibSlow(n - 1) + fibSlow(n - 2); }

    static int gcd(int a, int b) { return b == 0 ? a : gcd(b, a % b); }

    static boolean[] sieve(int n) {
        boolean[] composite = new boolean[n + 1];
        for (int i = 2; (long) i * i <= n; i++) {
            if (!composite[i]) {
                for (int j = i * i; j <= n; j += i) composite[j] = true;
            }
        }
        return composite;
    }

    static void quicksort(int[] a, int lo, int hi) {
        if (lo >= hi) return;
        int p = a[(lo + hi) >>> 1], i = lo, j = hi;
        while (i <= j) {
            while (a[i] < p) i++;
            while (a[j] > p) j--;
            if (i <= j) {
                int t = a[i]; a[i] = a[j]; a[j] = t;
                i++; j--;
            }
        }
        quicksort(a, lo, j);
        quicksort(a, i, hi);
    }

    static int binarySearch(int[] a, int key) {
        int lo = 0, hi = a.length - 1;
        while (lo <= hi) {
            int mid = (lo + hi) / 2;
            if (a[mid] < key) lo = mid + 1;
            else if (a[mid] > key) hi = mid - 1;
            else return mid;
        }
        return -(lo + 1);
    }

    static int queens(int n, int row, int cols, int d1, int d2) {
        if (row == n) return 1;
        int count = 0;
        for (int c = 0; c < n; c++) {
            if ((cols & (1 << c)) != 0 || (d1 & (1 << (row + c))) != 0 || (d2 & (1 << (row - c + n))) != 0) continue;
            count += queens(n, row + 1, cols | 1 << c, d1 | 1 << (row + c), d2 | 1 << (row - c + n));
        }
        return count;
    }

    static long[][] matmul(long[][] a, long[][] b) {
        int n = a.length;
        long[][] c = new long[n][n];
        for (int i = 0; i < n; i++)
            for (int j = 0; j < n; j++) {
                long s = 0;
                for (int k = 0; k < n; k++) s += a[i][k] * b[k][j];
                c[i][j] = s;
            }
        return c;
    }

    static String toRoman(int num) {
        int[] vals = {1000, 900, 500, 400, 100, 90, 50, 40, 10, 9, 5, 4, 1};
        String[] syms = {"M", "CM", "D", "CD", "C", "XC", "L", "XL", "X", "IX", "V", "IV", "I"};
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < vals.length; i++) {
            while (num >= vals[i]) {
                num -= vals[i];
                sb.append(syms[i]);
            }
        }
        return sb.toString();
    }

    static int lcs(String a, String b) {
        int[][] dp = new int[a.length() + 1][b.length() + 1];
        for (int i = 1; i <= a.length(); i++)
            for (int j = 1; j <= b.length(); j++)
                dp[i][j] = a.charAt(i - 1) == b.charAt(j - 1) ? dp[i - 1][j - 1] + 1 : Math.max(dp[i - 1][j], dp[i][j - 1]);
        return dp[a.length()][b.length()];
    }

    static void hanoi(int n, char from, char to, char via, int[] moves) {
        if (n == 0) return;
        hanoi(n - 1, from, via, to, moves);
        moves[0]++;
        hanoi(n - 1, via, to, from, moves);
    }

    public static void main(String[] args) {
        System.out.println(fib(90));
        System.out.println(fibSlow(20));
        System.out.println(gcd(1071, 462));
        boolean[] comp = sieve(100);
        StringBuilder primes = new StringBuilder();
        int pc = 0;
        for (int i = 2; i <= 100; i++) if (!comp[i]) { primes.append(i).append(' '); pc++; }
        System.out.println(pc + ": " + primes);

        int[] data = new int[20];
        long seed = 12345;
        for (int i = 0; i < data.length; i++) {
            seed = (seed * 1103515245 + 12345) & 0x7fffffffL;
            data[i] = (int) (seed % 1000);
        }
        int[] copy = Arrays.copyOf(data, data.length);
        quicksort(data, 0, data.length - 1);
        Arrays.sort(copy);
        System.out.println(Arrays.toString(data));
        System.out.println(Arrays.equals(data, copy));
        System.out.println(binarySearch(data, data[7]) + " " + binarySearch(data, -1));
        for (int n = 4; n <= 8; n++) System.out.print(queens(n, 0, 0, 0, 0) + " ");
        System.out.println();
        long[][] m = {{1, 1}, {1, 0}};
        long[][] r = {{1, 0}, {0, 1}};
        for (int i = 0; i < 50; i++) r = matmul(r, m);
        System.out.println(r[0][1] + " " + Arrays.deepToString(new int[][]{{1, 2}, {3}}));
        System.out.println(toRoman(1994) + " " + toRoman(2024) + " " + toRoman(3888));
        System.out.println(lcs("AGGTABQWERTY", "GXTXAYBWRT"));
        int[] moves = new int[1];
        hanoi(10, 'A', 'C', 'B', moves);
        System.out.println(moves[0]);
        int[][] grid = new int[5][];
        for (int i = 0; i < grid.length; i++) {
            grid[i] = new int[i + 1];
            grid[i][0] = grid[i][i] = 1;
            for (int j = 1; j < i; j++) grid[i][j] = grid[i - 1][j - 1] + grid[i - 1][j];
        }
        for (int[] row : grid) System.out.println(Arrays.toString(row));
        char[][] board = new char[3][3];
        for (char[] row : board) Arrays.fill(row, '.');
        board[1][1] = 'X';
        for (char[] row : board) System.out.println(new String(row));
        double[] ds = {3.5, -1, 2};
        Arrays.sort(ds);
        System.out.println(Arrays.toString(ds));
        String[] names = {"pear", "apple", "fig"};
        Arrays.sort(names);
        System.out.println(Arrays.toString(names) + " " + names.length);
    }
}
