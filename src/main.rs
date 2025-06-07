// Rust Basics Tutorial

/// A simple struct representing a person
#[derive(Debug)]
struct Person {
    name: String,
    age: u32,
}

// Add implementation block for Person
impl Person {
    fn new(name: String, age: u32) -> Person {
        Person { name, age }
    }

    fn describe(&self) -> String {
        format!("{} is {} years old", self.name, self.age)
    }
}

/// Adds two i32 numbers and returns their sum
fn add(a: i32, b: i32) -> i32 {
    a + b  // Implicit return (no semicolon)
}

/// Calculates the length of a string slice
fn calculate_length(s: &String) -> usize {
    s.len()
}

/// Custom error enum for demonstration
#[derive(Debug)]
enum CustomError {
    InvalidValue,
}

/// Function that might fail
fn divide(a: f64, b: f64) -> Result<f64, CustomError> {
    if b == 0.0 {
        Err(CustomError::InvalidValue)
    } else {
        Ok(a / b)
    }
}

fn main() {
    // 1. Variables and Mutability
    let mut counter = 0;
    counter += 5;
    println!("Counter value: {}", counter);

    // 2. Basic Data Types
    let integer: i32 = 42;
    let float: f64 = 3.14;
    let boolean: bool = true;
    let character: char = 'A';
    let text: &str = "Hello, Rust!";
    
    // Print basic types
    println!("\nBasic Types:");
    println!("Integer: {}", integer);
    println!("Float: {}", float);
    println!("Boolean: {}", boolean);
    println!("Character: {}", character);
    println!("Text: {}", text);
    
    // 3. Compound Types
    let array: [i32; 5] = [1, 2, 3, 4, 5];
    let mut vector: Vec<i32> = vec![1, 2, 3];
    vector.push(4);
    println!("Vector contents: {:?}", vector);

    let tuple: (i32, f64, &str) = (1, 2.0, "three");
    println!("\nTuple values: ({}, {}, {})", tuple.0, tuple.1, tuple.2);

    // 4. Control Flow
    let number = 7;
    if number < 5 {
        println!("number is less than 5");
    } else {
        println!("number is 5 or greater");
    }

    // For loop
    println!("\nArray values:");
    for num in array.iter() {
        println!("Number: {}", num);
    }
    
    // 5. Ownership and Borrowing
    let s1 = String::from("hello");
    let s2 = s1.clone(); // Deep copy
    let len = calculate_length(&s2); // Borrowing

    // 6. Using the Person struct with implementation
    let person = Person::new(String::from("Alice"), 30);
    println!("\nPerson description: {}", person.describe());
    println!("Person debug view: {:?}", person);

    // 7. Error Handling
    match divide(10.0, 2.0) {
        Ok(result) => println!("\nDivision result: {}", result),
        Err(e) => println!("Error occurred: {:?}", e),
    }

    // Print results
    println!("\nBasic Rust Examples:");
    println!("Addition: {}", add(5, 3));
    println!("String length: {}", len);
    
    // Using if let for cleaner error handling
    if let Ok(result) = divide(10.0, 2.0) {
        println!("Safe division result: {}", result);
    }
}