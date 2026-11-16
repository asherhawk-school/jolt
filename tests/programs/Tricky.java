interface Greeter {
    String greet(String who);
}

class Polite implements Greeter {
    public String greet(String who) { return "Good day, " + who; }
}

class Casual implements Greeter {
    public String greet(String who) { return "hey " + who; }
}

class Animal {
    protected String sound = "...";
    protected int legs = 4;
    String describe() { return "animal with " + legs + " legs says " + sound; }
    @Override
    public boolean equals(Object o) {
        if (!(o instanceof Animal)) return false;
        Animal a = (Animal) o;
        return a.legs == legs && a.sound.equals(sound);
    }
    @Override
    public String toString() { return "Animal(" + sound + ")"; }
}

class Bird extends Animal {
    protected String sound = "tweet";
    Bird() { legs = 2; super.sound = "chirp"; }
    String describe() { return super.describe() + " / own sound " + sound + " / super " + super.sound; }
}

public class Tricky {
    private static int secret = 41;
    private static String hidden() { return "hidden!"; }
    int value;
    String text = "t";
    Tricky self;
    static int calls;

    static class Inner {
        static String peek() { return hidden() + (secret + 1); }
    }

    Tricky(int value) { this.value = value; self = this; }

    Tricky get() { calls++; return this; }

    static String f(int x) { return "int"; }
    static String f(long x) { return "long"; }
    static String f(double x) { return "double"; }
    static String f(Object x) { return "Object"; }
    static String f(String x) { return "String"; }
    static String g(long x) { return "g-long"; }
    static String g(double x) { return "g-double"; }

    static boolean t(String s) { System.out.print(s); return true; }
    static boolean fl(String s) { System.out.print(s); return false; }

    static int sumAll(int... nums) {
        int s = 0;
        for (int n : nums) s += n;
        return s;
    }

    static String describe(Greeter g, String who) { return g.greet(who); }

    public static void main(String[] args) {
        System.out.println(Inner.peek());
        Greeter[] gs = {new Polite(), new Casual()};
        for (Greeter g : gs) System.out.println(describe(g, "Ada"));
        Greeter gg = gs[1];
        System.out.println(gg.greet("Bob") + " " + gg.toString().startsWith("Casual"));

        Bird b = new Bird();
        System.out.println(b.describe());
        Animal a = b;
        System.out.println(a.sound + " " + a.legs + " " + a);
        Animal x = new Animal();
        System.out.println(x.equals(new Animal()) + " " + x.equals(b) + " " + x.equals("nope"));

        System.out.println(f(1) + " " + f(1L) + " " + f(1.0) + " " + f('c') + " " + f("s") + " " + f(new int[0]) + " " + f(null == null ? "a" : "b"));
        System.out.println(g(3) + " " + g('x') + " " + g(2.0));

        Tricky tr = new Tricky(5);
        tr.get().value += 10;
        tr.get().text += "+" + tr.value;
        tr.self.self.value++;
        System.out.println(tr.value + " " + tr.text + " " + calls);
        int[] arr = new int[5];
        int i = 0;
        arr[i++] += 2;
        arr[i++] = arr[0] * 3;
        arr[++i] -= 7;
        System.out.println(java.util.Arrays.toString(arr) + " i=" + i);
        secret++;
        Tricky.secret += 100;
        System.out.println(secret);

        if (fl("A") && t("B")) System.out.print("X");
        if (t("C") || t("D")) System.out.print("Y");
        if (fl("E") || fl("F") || t("G")) System.out.print("Z");
        boolean r = t("H") && (fl("I") || t("J")) && !fl("K");
        System.out.println(" " + r);

        long big1 = 3000000000L, big2 = 3000000001L;
        if (big1 < big2 && big2 > big1 && big1 != big2 && !(big1 == big2) && big1 <= big1 && big2 >= big1)
            System.out.println("long compares ok");
        double nan = Double.NaN;
        if (nan < 0 || nan > 0 || nan == nan || nan <= 0 || nan >= 0) System.out.println("bad nan");
        else System.out.println("nan compares ok");
        if (!(nan < 0)) System.out.println("negated nan ok");

        final int base = 10;
        int value = base * 2;
        System.out.println(value + " " + tr.value);

        int[][][] cube = new int[2][3][4];
        cube[1][2][3] = 42;
        System.out.println(cube.length + " " + cube[0].length + " " + cube[0][0].length + " " + cube[1][2][3]);
        Tricky[] objs = new Tricky[3];
        objs[1] = tr;
        for (Tricky o : objs) System.out.print((o == null ? "null" : "T" + o.value) + " ");
        System.out.println();

        System.out.println(-7 / 2 + " " + -7 % 2 + " " + Math.floorMod(-7, 2) + " " + Math.floorDiv(-7, 2));
        System.out.println(46341 * 46341 + " " + 46341L * 46341);

        int n = 0;
        outer:
        do {
            n++;
            for (int k = 0; k < 3; k++) {
                if (k == n) continue outer;
            }
        } while (n < 5);
        System.out.println("n=" + n);

        String s = null;
        System.out.println(s == null ? "none" : s.length() + "");
        System.out.println(sumAll() + " " + sumAll(1) + " " + sumAll(1, 2, 3, 4) + " " + sumAll(new int[]{5, 5}));

        StringBuilder sb = new StringBuilder();
        for (char c = 'z'; c >= 'a'; c -= 5) sb.append(c);
        System.out.println(sb);

        int count = 0;
        for (int p = 0; p < 1000; p++) {
            boolean even = (p & 1) == 0;
            boolean div3 = p % 3 == 0;
            if (even ^ div3) count++;
        }
        System.out.println(count);
        Object o = count > 5 ? "big" : null;
        System.out.println(o);
        char grade = 'B';
        switch (grade) {
            case 'A': System.out.println("top"); break;
            case 'B':
            case 'C': System.out.println("mid"); break;
            default: System.out.println("low");
        }
        System.out.println(Integer.MAX_VALUE + Integer.MAX_VALUE);
        System.out.println((byte_like(300)) + " " + (char) 65 + (char) 97);
        long acc = 1;
        for (int k = 1; k <= 20; k++) acc *= k;
        System.out.println("20! = " + acc);
        System.out.println(Long.MAX_VALUE / -1 + " " + Long.MIN_VALUE / -1);
    }

    static int byte_like(int v) { return (v << 24) >> 24; }
}
