use smallvec::*;
use std::collections::HashMap;

pub mod ir;
pub mod parse;
use ir::*;

impl Node {
    pub fn mangle(self, m: &mut Module, map: &HashMap<Val, Val>) -> Self {
        match self {
            Node::Fun(mut f) => {
                f.params.iter_mut().for_each(|x| *x = x.mangle(m, map));
                f.callee = f.callee.mangle(m, map);
                f.call_args.iter_mut().for_each(|x| *x = x.mangle(m, map));
                Node::Fun(f)
            }
            Node::Param(f, i) => Node::Param(f.mangle(m, map), i),
            Node::BinOp(op, a, b) => Node::BinOp(op, a.mangle(m, map), b.mangle(m, map)),
            // Constants can't use other values
            Node::Const(c) => Node::Const(c),
        }
    }
}
impl Val {
    pub fn mangle(self, m: &mut Module, map: &HashMap<Val, Val>) -> Self {
        if let Some(&x) = map.get(&self) {
            x
        } else {
            if let Some(new) = m.get(self).cloned().map(|x| x.mangle(m, map)) {
                m.add(new)
            } else {
                // Assume it doesn't use things in `map`, I guess?
                self
            }
        }
    }
}

pub fn lift(m: &mut Module, vfun: Val, to_lift: Val) -> Val {
    // Clone the function
    let fun = m.get(vfun).expect("lift() on nonexistent function").clone();
    let mut fun = if let Node::Fun(fun) = fun {
        fun
    } else {
        panic!("lift() on non-function")
    };

    // Add the new parameter
    let ty = m
        .get(to_lift)
        .unwrap()
        .ty(m)
        .right_or_else(|x| m.add(Node::Const(x)));
    let nparams = fun.params.len();
    fun.params.push(ty);

    // Update references to the old function's parameters in the body of `fun` to the new parameters
    let fnew = m.reserve();
    let lifted = m.add(Node::Param(fnew, nparams as u8));
    let mut map = HashMap::new();
    // We're modifying `m` in the loop, so we need to `clone()` here
    for i in m.uses(vfun).clone() {
        let new = match m.get(i).unwrap() {
            // A known call of the function should be switch to calling the new one
            Node::Fun(f) if f.callee == vfun => {
                // To call the new function, though, it needs to pass it the new parameter
                let mut call_args = f.call_args.clone();
                call_args.push(lifted);
                Some(Node::Fun(Function {
                    params: f.params.clone(),
                    callee: fnew,
                    call_args,
                }))
            }
            // If it's referencing a parameter, we just switch the function
            Node::Param(f, i) => {
                assert_eq!(*f, vfun);
                Some(Node::Param(fnew, *i))
            }
            _ => None,
        };
        if let Some(new) = new {
            let new = m.add(new);
            map.insert(i, new);
        }
    }

    // Update references to `to_lift` to the new parameter
    map.insert(to_lift, lifted);

    // Actually apply both replacements
    let fun = Node::Fun(fun).mangle(m, &map);
    m.replace(fnew, fun);
    fnew
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifting() {
        let mut m = Module::default();
        let (before, to_lift) = {
            let u32_t = m.add(Node::Const(Constant::IntType(Width::W32)));
            let enclosing = m.reserve();
            // TODO working forward references
            let param_0 = m.add(Node::Param(enclosing, 0));

            let before = m.add(Node::Fun(Function {
                params: SmallVec::new(),
                callee: enclosing,
                call_args: smallvec![param_0],
            }));
            m.replace(
                enclosing,
                Node::Fun(Function {
                    params: smallvec![u32_t],
                    callee: before,
                    call_args: SmallVec::new(),
                }),
            );
            (before, param_0)
        };

        // So we can see in the output where the lifting happened
        m.add(Node::Const(Constant::Int(Width::W64, 111111111111)));

        let after = lift(&mut m, before, to_lift);
        for (i, n) in m.nodes.iter().enumerate() {
            println!("%{} = {}", i, n.as_ref().unwrap());
        }
        // panic!("look right?");
    }
}