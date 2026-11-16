// expect: Dog is not abstract and does not override abstract method speak() in Animal
abstract class Animal { abstract String speak(); }
class Dog extends Animal { }
public class Abstract { public static void main(String[] args) {} }
