public class Statics {
    static final int SIZE = 4;
    static final long BIG = SIZE * 1000000000L;
    static final double HALF = 1 / 2.0;
    static final String GREETING = "hi" + SIZE;
    static final boolean DEBUG = SIZE > 10;
    static int[] table = new int[SIZE];
    static int[][] grid = {{1, 2}, {3, 4}};
    static String log = "";
    static int initOrder;

    static {
        for (int i = 0; i < SIZE; i++) table[i] = i * i;
        log += "static-block ";
        initOrder = 1;
    }

    int a = 5;
    int b = a * 2;
    String tag;
    {
        tag = "init-block";
        log += "instance-block ";
    }

    Statics() {
        log += "ctor ";
    }

    Statics(int x) {
        this();
        a = x;
    }

    public static void main(String[] args) {
        System.out.println(SIZE + " " + BIG + " " + HALF + " " + GREETING + " " + DEBUG);
        System.out.println(table[3] + " " + grid[1][0] + " " + initOrder);
        Statics s = new Statics(9);
        System.out.println(s.a + " " + s.b + " " + s.tag);
        System.out.println(log);
        if (DEBUG) System.out.println("debug on");
        switch (7) {
            case SIZE: System.out.println("four"); break;
            case SIZE + 3: System.out.println("seven"); break;
        }
        System.out.println(Character.MAX_VALUE + 0);
        System.out.println(Integer.MIN_VALUE + " " + Long.MAX_VALUE + " " + Double.MAX_VALUE);
    }
}
