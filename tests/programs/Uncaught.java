public class Uncaught {
    static class Account {
        int balance;
        void withdraw(int n) {
            if (n > balance) throw new IllegalStateException("insufficient funds: " + balance);
            balance -= n;
        }
    }
    static int depth = 0;
    static void recurse() { depth++; recurse(); }

    public static void main(String[] args) {
        try {
            recurse();
        } catch (StackOverflowError e) {
            System.out.println("stack overflow caught: " + (depth > 1000));
        }
        try {
            throw new RuntimeException("outer", new IllegalArgumentException("inner cause"));
        } catch (RuntimeException e) {
            System.out.println(e + " / cause: " + e.getCause().getMessage());
        }
        int[] data = {1, 2, 3};
        System.out.println(data.length);
        Account a = new Account();
        a.balance = 10;
        a.withdraw(4);
        System.out.println("balance " + a.balance);
        a.withdraw(100);
        System.out.println("unreachable");
    }
}
