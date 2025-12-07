//! Domain-style enums with methods.

#[derive(Debug)]
enum OrderState {
    Pending { id: u32 },
    Shipped { id: u32, carrier: String },
    Cancelled { id: u32, reason: String },
}

impl OrderState {
    fn label(&self) -> String {
        match self {
            OrderState::Pending { id } => format!("order {id} is pending"),
            OrderState::Shipped { id, carrier } => format!("order {id} shipped via {carrier}"),
            OrderState::Cancelled { id, reason } => {
                format!("order {id} cancelled: {reason}")
            }
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(self, OrderState::Shipped { .. } | OrderState::Cancelled { .. })
    }
}

#[derive(Debug)]
enum Message {
    Ping,
    Text(String),
    Move(i32, i32),
    Batch(Vec<Message>),
}

fn describe_message(msg: Message) -> String {
    match msg {
        Message::Ping => "ping".to_string(),
        // Match guard shows conditional logic on a variant.
        Message::Text(body) if body.len() > 10 => format!("text(long): {}", body.len()),
        Message::Text(body) => format!("text: {body}"),
        Message::Move(x, y) => format!("move to ({x}, {y})"),
        // Nested data: Vec of other messages.
        Message::Batch(items) => format!("batch of {}", items.len()),
    }
}

pub fn enums_demo() -> String {
    let pending = OrderState::Pending { id: 1 };
    let shipped = OrderState::Shipped {
        id: 2,
        carrier: "orbital".to_string(),
    };
    let cancelled = OrderState::Cancelled {
        id: 3,
        reason: "payment failed".to_string(),
    };

    let messages = vec![
        Message::Ping,
        Message::Text("hi".into()),
        Message::Text("this is a longer message".into()),
        Message::Move(3, 4),
        Message::Batch(vec![Message::Ping, Message::Move(0, 1)]),
    ];

    let lines = vec![
        pending.label(),
        format!("pending terminal: {}", pending.is_terminal()),
        shipped.label(),
        format!("shipped terminal: {}", shipped.is_terminal()),
        cancelled.label(),
        describe_message(messages[0].clone()),
        describe_message(messages[1].clone()),
        describe_message(messages[2].clone()),
        describe_message(messages[3].clone()),
        describe_message(messages[4].clone()),
    ];

    lines.join("\n")
}

