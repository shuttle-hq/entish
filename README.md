**This crate is currently under active development and is not ready for use yet.**

# Entish

> You must understand, young Hobbit, it takes a long time to say anything in Old Entish. And we never say anything unless it is worth taking a long time to say.

**Entish** is an attempt to expose the code-generation around the common boilerplate needed to work with **trees** in safe Rust. It should be easy to build statically typed, complex, tree structures and at the same time not sacrifice readability or maintainability.

![ferris](./resources/ferris-entish.png)

## Origin

At [OpenQuery](https://openquery.io) we are building a distributed data lake driver. And that required us to take a new approach to query optimization, which involves a lot of vast and complex statically typed trees (think SQL11 AST with lots of labels). Being big Rust fans we were surprised to not find a good idiomatic crate to do common things such as iterating on children, recursive descent with failures, etc. So we set out writing our own! We want to give back to the community by extracting as much of that internal code as we can into this project.

## Example

```rust
#[macro_use] extern crate entish;
use entish::prelude::*;

entish! {
    #[entish(variants_as_structs)]
    #[derive(Map, From)]
    enum Arithmetic<U> {
        Plus {
            left: Self,
            right: Self
        },
        Times {
            left: Self,
            right: Self
        },
        Just(U)
    }
}
```

Wrapping in the `entish! { ... }` macro rewrites the `Arithmetic` enum by adding a dummy generic parameter `__Child` and replaces the inner `Self` fields by `__Child`. The attributes added to the enum customize the behavior of the underlying codegen:
- The `#[entish(variants_as_structs)]` attribute forces rewriting the enum by replacing all variants by unnamed variants and declaring new structs,
- The `#[derive(Map, From)]` attribute is consumed by Entish and impl's 
  - for `From`: conversion from the structs declared by `variants_as_structs`
  - for `Map`: enables the use of `.map`, which takes a closure `FnMut(&Child) -> O` and a node of type `Arithmetic<U, Child>` and yields a node of type `Arithmetic<U, O>`. In plain English: applies a closure to the children of a node.

Finally, it generates a trait 
```rust
trait ArithmeticTree<U>: Sized
where
    U: Clone,
{
    fn as_ref(&self) -> Arithmetic<U, &Self>;
    ...
}
```
and a couple useful functions (such as `fold` and `iter_children`).

## To Do's

We are currently in the process of extracting our internal version of Entish into an easy to use, general purpose, tree crate.

## License

**entish** is licensed under the Apache License, Version 2.0. See [LICENSE](./LICENSE) for the full license text.

## Contribution

See [Contributing](./CONTRIBUTING.md).


