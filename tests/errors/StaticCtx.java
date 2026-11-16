// expect: non-static variable value cannot be referenced from a static context
public class StaticCtx {
    int value;
    public static void main(String[] args) {
        value = 3;
    }
}
