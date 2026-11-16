interface Shape {
    double area();
    String name();
}

interface Scalable {
    void scale(double f);
}

abstract class Base implements Shape, Scalable {
    private static int created = 0;
    protected final int id;

    Base() {
        created++;
        id = created;
    }

    static int count() { return created; }

    public String name() { return getClass_() + "#" + id; }

    abstract String getClass_();

    @Override
    public String toString() {
        return name() + String.format("[area=%.2f]", area());
    }
}

class Circle extends Base {
    private double r;
    Circle(double r) { super(); this.r = r; }
    public double area() { return Math.PI * r * r; }
    public void scale(double f) { r *= f; }
    String getClass_() { return "Circle"; }
}

class Rect extends Base {
    protected double w, h;
    Rect(double w, double h) { this.w = w; this.h = h; }
    Rect(double side) { this(side, side); }
    public double area() { return w * h; }
    public void scale(double f) { w *= f; h *= f; }
    String getClass_() { return "Rect"; }
}

class Square extends Rect {
    Square(double s) { super(s); }
    @Override
    String getClass_() { return "Square"; }
    @Override
    public String name() { return "Sq(" + super.name() + ")"; }
}

public class Shapes {
    static class Counter {
        private int value;
        private static int instances;
        Counter() { instances++; }
        Counter inc() { value++; return this; }
        int get() { return value; }
    }

    static class Node {
        int val;
        Node next;
        Node(int val, Node next) { this.val = val; this.next = next; }
    }

    static class Stack {
        private Node head;
        private int size;
        void push(int v) { head = new Node(v, head); size++; }
        int pop() {
            if (head == null) throw new IllegalStateException("empty stack");
            int v = head.val;
            head = head.next;
            size--;
            return v;
        }
        boolean isEmpty() { return head == null; }
        int size() { return size; }
    }

    static class Tree {
        static class TNode {
            int key;
            TNode left, right;
            TNode(int key) { this.key = key; }
        }
        TNode root;
        void insert(int k) { root = insert(root, k); }
        private TNode insert(TNode n, int k) {
            if (n == null) return new TNode(k);
            if (k < n.key) n.left = insert(n.left, k);
            else if (k > n.key) n.right = insert(n.right, k);
            return n;
        }
        void inorder(TNode n, StringBuilder sb) {
            if (n == null) return;
            inorder(n.left, sb);
            sb.append(n.key).append(' ');
            inorder(n.right, sb);
        }
        int height(TNode n) { return n == null ? 0 : 1 + Math.max(height(n.left), height(n.right)); }
    }

    public static void main(String[] args) {
        Shape[] shapes = { new Circle(1), new Rect(2, 3), new Square(4), new Circle(0.5) };
        double total = 0;
        for (Shape s : shapes) {
            System.out.println(s);
            total += s.area();
        }
        System.out.printf("total=%.3f count=%d%n", total, Base.count());
        for (Shape s : shapes) {
            if (s instanceof Scalable) ((Scalable) s).scale(2);
        }
        for (Shape s : shapes) System.out.println(s.name() + " -> " + Math.round(s.area()));
        Object o = shapes[2];
        System.out.println(o instanceof Rect);
        System.out.println(o instanceof Circle);
        Rect r = (Rect) o;
        System.out.println(r.w + " x " + r.h);

        Counter c = new Counter();
        c.inc().inc().inc();
        new Counter();
        System.out.println(c.get() + " " + Counter.instances);

        Stack st = new Stack();
        for (int i = 1; i <= 5; i++) st.push(i * i);
        System.out.print(st.size() + ":");
        while (!st.isEmpty()) System.out.print(" " + st.pop());
        System.out.println();
        try {
            st.pop();
        } catch (IllegalStateException e) {
            System.out.println("caught: " + e.getMessage());
        }

        Tree t = new Tree();
        int[] keys = {50, 30, 70, 20, 40, 60, 80, 35, 45, 65};
        for (int k : keys) t.insert(k);
        StringBuilder sb = new StringBuilder();
        t.inorder(t.root, sb);
        System.out.println(sb.toString().trim());
        System.out.println("height " + t.height(t.root));

        Object[] things = { "str", 42, 'c', 2.5, true, null, new int[0].length };
        for (Object th : things) System.out.print(th + " ");
        System.out.println();
        String s1 = "hello";
        String s2 = "hel" + "lo";
        System.out.println(s1 == s2);
        System.out.println(s1.equals(new String("hello")));
        System.out.println(s1.hashCode() == s2.hashCode());
    }
}
