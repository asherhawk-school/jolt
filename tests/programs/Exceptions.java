public class Exceptions {
    static class InsufficientFunds extends RuntimeException {
        final int needed;
        InsufficientFunds(String msg, int needed) {
            super(msg);
            this.needed = needed;
        }
    }

    static class Account {
        private int balance;
        Account(int b) { balance = b; }
        void withdraw(int amt) {
            if (amt <= 0) throw new IllegalArgumentException("amount must be positive: " + amt);
            if (amt > balance) throw new InsufficientFunds("need more money", amt - balance);
            balance -= amt;
        }
        int balance() { return balance; }
    }

    static int depth(int n) {
        try {
            if (n == 0) throw new RuntimeException("bottom");
            return depth(n - 1) + 1;
        } catch (ArithmeticException e) {
            return -1;
        }
    }

    static int safeDiv(int a, int b) {
        try {
            return a / b;
        } catch (ArithmeticException e) {
            System.out.println("caught " + e.getMessage());
            return 0;
        }
    }

    public static void main(String[] args) {
        Account acc = new Account(100);
        int[] tries = {30, -5, 200, 50};
        for (int t : tries) {
            try {
                acc.withdraw(t);
                System.out.println("withdrew " + t + ", balance " + acc.balance());
            } catch (InsufficientFunds e) {
                System.out.println(e.getMessage() + ": short by " + e.needed);
            } catch (IllegalArgumentException e) {
                System.out.println("bad: " + e.getMessage());
            }
        }
        System.out.println(safeDiv(10, 2));
        System.out.println(safeDiv(1, 0));
        try {
            depth(3);
        } catch (RuntimeException e) {
            System.out.println("depth threw " + e.getMessage());
        }
        try {
            int[] a = new int[2];
            a[5] = 1;
        } catch (ArrayIndexOutOfBoundsException | NullPointerException e) {
            System.out.println("multi-catch: " + (e instanceof ArrayIndexOutOfBoundsException));
        }
        try {
            String s = null;
            s.length();
        } catch (NullPointerException e) {
            System.out.println("npe");
        }
        try {
            Object o = "str";
            Integer i = (Integer) o;
            System.out.println(i);
        } catch (ClassCastException e) {
            System.out.println("cce");
        }
        try {
            Integer.parseInt("abc");
        } catch (NumberFormatException e) {
            System.out.println(e.getMessage());
        }
        try {
            try {
                throw new IllegalStateException("inner");
            } catch (IllegalArgumentException e) {
                System.out.println("wrong handler");
            }
        } catch (IllegalStateException e) {
            System.out.println("outer caught " + e.getMessage());
        }
        int count = 0;
        for (int i = 0; i < 5; i++) {
            try {
                if (i % 2 == 0) throw new Exception("e" + i);
                count += 10;
            } catch (Exception e) {
                count++;
                if (i == 4) break;
                continue;
            }
            count += 100;
        }
        System.out.println(count);
        try {
            throw new Error("custom error");
        } catch (Throwable t) {
            System.out.println(t.getMessage());
        }
    }
}
