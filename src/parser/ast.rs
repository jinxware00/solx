//! Abstract syntax tree for Solx.

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeclarationKind {
    Var,
    Global,
    Const,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Import {
        specifier: String,
        name: String,
    },
    Function {
        name: String,
        function: Function,
    },
    Declaration {
        kind: DeclarationKind,
        name: String,
        type_annotation: Option<String>,
        initializer: Option<Expr>,
    },
    Assignment {
        target: Expr,
        value: Expr,
    },
    Expression(Expr),
    Return(Vec<Expr>),
    Break,
    Continue,
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    NumericFor {
        name: String,
        start: Expr,
        limit: Expr,
        step: Option<Expr>,
        body: Vec<Stmt>,
    },
    If {
        branches: Vec<(Expr, Vec<Stmt>)>,
        else_branch: Option<Vec<Stmt>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub parameters: Vec<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Negate,
    Not,
    Length,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    FloorDivide,
    Modulo,
    Power,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    Concatenate,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Function(Function),
    Nil,
    Boolean(bool),
    Number(String),
    String(String),
    Identifier(String),

    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },

    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },

    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },

    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },

    Property {
        object: Box<Expr>,
        name: String,
    },

    List(Vec<Expr>),

    Object(Vec<(String, Expr)>),
}
