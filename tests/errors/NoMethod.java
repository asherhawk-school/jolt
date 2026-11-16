// expect: no suitable method found for max(String,int)
public class NoMethod {
    public static void main(String[] args) {
        System.out.println(Math.max("a", 1));
    }
}
