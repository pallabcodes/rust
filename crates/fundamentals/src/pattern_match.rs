//! Covers match and if-let.

enum Shape {
    Circle(f64),
    Rectangle { width: f64, height: f64 },
}

fn area(shape: Shape) -> f64 {
    match shape {
        Shape::Circle(r) => std::f64::consts::PI * r * r,
        Shape::Rectangle { width, height } => width * height,
    }
}

fn classify(value: Option<i32>) -> String {
    if let Some(num) = value {
        format!("got {num}")
    } else {
        "got nothing".to_string()
    }
}

pub fn patterns_demo() -> String {
    let circle_area = area(Shape::Circle(2.0));
    let rect_area = area(Shape::Rectangle {
        width: 3.0,
        height: 4.0,
    });

    let some_text = classify(Some(7));
    let none_text = classify(None);

    let lines = vec![
        format!("circle area: {:.2}", circle_area),
        format!("rectangle area: {:.2}", rect_area),
        some_text,
        none_text,
    ];

    lines.join("\n")
}

