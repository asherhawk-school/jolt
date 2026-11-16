// expect: unreachable statement
public class Unreachable {
    public static void main(String[] args) {
        return;
        System.out.println("never");
    }
}
