public class Arith {
    static int counter = 0;
    static long big = 1L << 40;
    static final int K = 10 * 3 + 2;
    static final String NAME = "arith" + K;
    static final char C = 'a' + 2;

    static int next() { return ++counter; }

    public static void main(String[] args) {
        int a = 17, b = 5;
        System.out.println(a + b);
        System.out.println(a - b);
        System.out.println(a * b);
        System.out.println(a / b);
        System.out.println(a % b);
        System.out.println(-a / b);
        System.out.println(-a % b);
        System.out.println(a << 3);
        System.out.println(-a >> 2);
        System.out.println(-a >>> 28);
        System.out.println(a & b);
        System.out.println(a | b);
        System.out.println(a ^ b);
        System.out.println(~a);
        System.out.println(Integer.MAX_VALUE + 1);
        System.out.println(Integer.MIN_VALUE - 1);
        System.out.println(-2147483648);
        System.out.println(0x7fffffff);
        System.out.println(0xFFFFFFFF);
        System.out.println(0b1011);
        System.out.println(017);
        System.out.println(1_000_000);

        long l = 123456789012L;
        System.out.println(l * 1000);
        System.out.println(l / 7);
        System.out.println(l % 7);
        System.out.println(l << 3);
        System.out.println(l >> 3);
        System.out.println(-l >>> 60);
        System.out.println(big);
        System.out.println(Long.MAX_VALUE);
        System.out.println(Long.MIN_VALUE);
        System.out.println(-9223372036854775808L);
        System.out.println(a + l);

        double d = 3.75;
        System.out.println(d * 2);
        System.out.println(d / 0.5);
        System.out.println(d % 1.5);
        System.out.println(1.0 / 3);
        System.out.println(10 / 4.0);
        System.out.println(1e10);
        System.out.println(1.5e-7);
        System.out.println(.5);
        System.out.println(100.0);
        System.out.println(0.1 + 0.2);
        System.out.println(-0.0);
        System.out.println(1.0 / 0);
        System.out.println(0.0 / 0 == 0.0 / 0);
        double nan = 0.0 / 0;
        System.out.println(nan < 1);
        System.out.println(nan > 1);
        System.out.println(nan != nan);
        System.out.println(!(nan >= 1));

        // casts
        System.out.println((int) 3.99);
        System.out.println((int) -3.99);
        System.out.println((long) 1e19);
        System.out.println((int) 1e10);
        System.out.println((char) 66);
        System.out.println((int) 'z');
        System.out.println((char) ('a' + 1));
        System.out.println((int) 123456789012L);
        System.out.println((double) 7 / 2);
        System.out.println((int) Double.NaN);

        // char arithmetic
        char c = 'x';
        c++;
        System.out.println(c);
        c += 1;
        System.out.println(c);
        System.out.println(c + 1);
        System.out.println('a' + 'b');
        System.out.println("" + 'a' + 'b');
        System.out.println(C);

        // compound assignment & inc/dec
        int x = 10;
        x += 5; System.out.println(x);
        x -= 3; System.out.println(x);
        x *= 4; System.out.println(x);
        x /= 5; System.out.println(x);
        x %= 4; System.out.println(x);
        x <<= 4; System.out.println(x);
        x >>= 1; System.out.println(x);
        x |= 3; System.out.println(x);
        x &= 6; System.out.println(x);
        x ^= 5; System.out.println(x);
        x += 3.7; System.out.println(x);
        int y = x++ + ++x;
        System.out.println(x + " " + y);
        y = x-- - --x;
        System.out.println(x + " " + y);
        long ll = 5;
        ll += 10;
        ll++;
        System.out.println(ll);
        double dd = 1.5;
        dd++;
        dd *= 2;
        System.out.println(dd);

        int[] arr = {1, 2, 3};
        arr[1] += 10;
        arr[2]++;
        int old = arr[0]++;
        System.out.println(old + " " + arr[0] + " " + arr[1] + " " + arr[2]);
        long[] la = new long[2];
        la[0] += 5;
        la[1] = la[0]++ * 3;
        System.out.println(la[0] + " " + la[1]);

        counter = 0;
        int z = next() + next() * next();
        System.out.println(z);
        System.out.println(counter);

        int q = 5;
        q = q++ + q++;
        System.out.println(q);

        System.out.println(K + " " + NAME);
        System.out.println(Math.max(3, 7) + Math.min(-1L, 4L) + Math.abs(-2.5));
        System.out.println(Math.sqrt(144) + " " + Math.pow(2, 10) + " " + Math.round(2.5) + " " + Math.floor(-1.5));
        System.out.println(Math.PI);
        System.out.println(Integer.toBinaryString(42) + " " + Integer.toHexString(255) + " " + Integer.parseInt("-77"));
        System.out.println(Long.parseLong("9000000000") + 1);
        System.out.println(Double.parseDouble("2.5") * 2);
        System.out.println(Integer.MAX_VALUE * 2L);
        System.out.println(5 / 2 * 2.0);
        System.out.println(1 + 2 + "3" + 4 + 5);
        boolean t = true, f = false;
        System.out.println(t & f);
        System.out.println(t | f);
        System.out.println(t ^ f);
        System.out.println(t && !f);
        boolean bb = true;
        bb &= false;
        System.out.println(bb);
        bb |= true;
        System.out.println(bb);
    }
}
