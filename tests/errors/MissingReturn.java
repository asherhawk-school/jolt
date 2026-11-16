// expect: 5:5: error: missing return statement
public class MissingReturn {
    static int f(int x) {
        if (x > 0) return 1;
    }
    public static void main(String[] args) {}
}
