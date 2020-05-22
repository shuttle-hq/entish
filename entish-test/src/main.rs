#![feature(box_patterns, try_trait, proc_macro_hygiene)]

use std::ops::{Add, Mul};

#[macro_use] extern crate entish;
use entish::prelude::*;

entish! {
    #[derive(Map, MapOwned, From, IntoResult)]
    #[entish(variants_as_structs)]
    enum Arithmetic {
        Plus {
            left: Self,
            right: Self
        },
        Times {
            left: Self,
            right: Self
        },
        Just(i32)
    }
}

pub struct Expr(Arithmetic<Box<Self>>);

impl ArithmeticTree for Expr
{
    fn as_ref(&self) -> Arithmetic<&Self> {
        self.0.map(&mut |c| c.as_ref())
    }
}

fn do_arithmetic(node: &Arithmetic<i32>) -> i32 {
    match *op {
        Arithmetic::Plus(Plus { left, right }) => left + right,
        Arithmetic::Times(Times { left, right }) => left * right,
        Arithmetic::Just(Just(v)) => v
    }
}

impl Expr
{
    fn compute_value(self) -> i32 {
        self.fold(&mut |op| do_arithmetic)
    }
}

fn main() {
    // an_expr = 5 + (2 * 6)
    //               Plus
    //               +  +
    //               |  |
    //               |  |
    //               |  |
    // Just(5)<------+  +------>Times
    //                          +  +
    //                          |  |
    //                          |  |
    //                          |  |
    //            Just(2)<------+  +------>Just(6)
    let an_expr = Expr(
        Arithmetic::Plus(
            Plus {
                left: Box::new(
                    Expr(
                        Arithmetic::Just(Just(1)
                        )
                    )
                ),
                right: Box::new(
                    Expr(
                        Arithmetic::Times(
                            Times {
                                left: Box::new(
                                    Expr(
                                        Arithmetic::Just(Just(2))
                                    )
                                ),
                                right: Box::new(
                                    Expr(
                                        Arithmetic::Just(Just(6))
                                    )
                                )
                            }
                        )
                    )
                )
            }
        )
    );

    assert_eq!(13, an_expr.compute_value())
}
