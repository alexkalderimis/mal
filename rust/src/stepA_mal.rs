// support precompiled regexes in reader.rs
#![feature(phase)]
#[phase(plugin)]
extern crate regex_macros;
extern crate regex;

use std::collections::HashMap;
use std::os;

use types::{MalVal,MalRet,MalError,ErrString,ErrMalVal,err_str,
            Nil,False,Sym,List,Vector,Hash_Map,Func,MalFunc,
            symbol,_nil,string,list,vector,hash_map,malfunc,malfuncd};
use env::{Env,env_new,env_bind,env_root,env_find,env_set,env_get};
mod readline;
mod types;
mod reader;
mod printer;
mod env;
mod core;

// read
fn read(str: String) -> MalRet {
    reader::read_str(str)
}

// eval
fn is_pair(x: MalVal) -> bool {
    match *x {
        List(ref lst,_) | Vector(ref lst,_) => lst.len() > 0,
        _ => false,
    }
}

fn quasiquote(ast: MalVal) -> MalVal {
    if !is_pair(ast.clone()) {
        return list(vec![symbol("quote"), ast])
    }

    match *ast.clone() {
        List(ref args,_) | Vector(ref args,_) => {
            let ref a0 = args[0];
            match **a0 {
                Sym(ref s) => {
                    if s.to_string() == "unquote".to_string() {
                        let ref a1 = args[1];
                        return a1.clone();
                    }
                },
                _ => (),
            }
            if is_pair(a0.clone()) {
                match **a0 {
                    List(ref a0args,_) | Vector(ref a0args,_) => {
                        let a00 = a0args[0].clone();
                        match *a00 {
                            Sym(ref s) => {
                                if s.to_string() == "splice-unquote".to_string() {
                                    return list(vec![symbol("concat"),
                                                     a0args[1].clone(),
                                                     quasiquote(list(args.slice(1,args.len()).to_vec()))])
                                }
                            },
                            _ => (),
                        }
                    },
                    _ => (),
                }
            }
            let rest = list(args.slice(1,args.len()).to_vec());
            return list(vec![symbol("cons"),
                             quasiquote(a0.clone()),
                             quasiquote(rest)])
        },
        _ => _nil(), // should never reach
    }
}

fn is_macro_call(ast: MalVal, env: Env) -> bool {
    match *ast {
        List(ref lst,_) => {
            match *lst[0] {
                Sym(_) => {
                    if env_find(env.clone(), lst[0].clone()).is_some() {
                        match env_get(env, lst[0].clone()) {
                            Ok(f) => {
                                match *f {
                                    MalFunc(ref mfd,_) => {
                                        mfd.is_macro
                                    },
                                    _ => false,
                                }
                            },
                            _ => false,
                        }
                    } else {
                        false
                    }
                },
                _ => false,
            }
        },
        _ => false,
    }
}

fn macroexpand(mut ast: MalVal, env: Env) -> MalRet {
    while is_macro_call(ast.clone(), env.clone()) {
        let ast2 = ast.clone();
        let args = match *ast2 {
            List(ref args,_) => args,
            _ => break,
        };
        let ref a0 = args[0];
        let mf = match **a0 {
            Sym(_) => {
                match env_get(env.clone(), a0.clone()) {
                    Ok(mf) => mf,
                    Err(e) => return Err(e),
                }
            },
            _ => break,
        };
        match *mf {
            MalFunc(_,_) => {
                match mf.apply(args.slice(1,args.len()).to_vec()) {
                    Ok(r) => ast = r,
                    Err(e) => return Err(e),
                }
            },
            _ => break,
        }
    }
    Ok(ast)
}

fn eval_ast(ast: MalVal, env: Env) -> MalRet {
    let ast2 = ast.clone();
    match *ast2 {
    //match *ast {
        Sym(_) => {
            env_get(env.clone(), ast)
        },
        List(ref a,_) | Vector(ref a,_) => {
            let mut ast_vec : Vec<MalVal> = vec![];
            for mv in a.iter() {
                let mv2 = mv.clone();
                match eval(mv2, env.clone()) {
                    Ok(mv) => { ast_vec.push(mv); },
                    Err(e) => { return Err(e); },
                }
            }
            Ok(match *ast { List(_,_) => list(ast_vec),
                            _         => vector(ast_vec) })
        },
        Hash_Map(ref hm,_) => {
            let mut new_hm: HashMap<String,MalVal> = HashMap::new();
            for (key, value) in hm.iter() {
                match eval(value.clone(), env.clone()) {
                    Ok(mv) => { new_hm.insert(key.to_string(), mv); },
                    Err(e) => return Err(e),
                }
            }
            Ok(hash_map(new_hm))
        },
        _ => {
            Ok(ast)
        }
    }
}

fn eval(mut ast: MalVal, mut env: Env) -> MalRet {
    'tco: loop {

    //println!("eval: {}, {}", ast, env.borrow());
    //println!("eval: {}", ast);
    let mut ast2 = ast.clone();
    match *ast2 {
        List(_,_) => (),  // continue
        _ => return eval_ast(ast2, env),
    }

    // apply list
    match macroexpand(ast2, env.clone()) {
        Ok(a) => {
            ast2 = a;
        },
        Err(e) => return Err(e),
    }
    match *ast2 {
        List(_,_) => (),  // continue
        _ => return Ok(ast2),
    }
    let ast3 = ast2.clone();

    let (args, a0sym) = match *ast2 {
        List(ref args,_) => {
            if args.len() == 0 { 
                return Ok(ast3);
            }
            let ref a0 = *args[0];
            match *a0 {
                Sym(ref a0sym) => (args, a0sym.as_slice()),
                _ => (args, "__<fn*>__"),
            }
        },
        _ => return err_str("Expected list"),
    };

    match a0sym {
        "def!" => {
            let a1 = (*args)[1].clone();
            let a2 = (*args)[2].clone();
            let res = eval(a2, env.clone());
            match res {
                Ok(r) => {
                    match *a1 {
                        Sym(_) => {
                            env_set(&env.clone(), a1.clone(), r.clone());
                            return Ok(r);
                        },
                        _ => {
                            return err_str("def! of non-symbol")
                        }
                    }
                },
                Err(e) => return Err(e),
            }
        },
        "let*" => {
            let let_env = env_new(Some(env.clone()));
            let a1 = (*args)[1].clone();
            let a2 = (*args)[2].clone();
            match *a1 {
                List(ref binds,_) | Vector(ref binds,_) => {
                    let mut it = binds.iter();
                    while it.len() >= 2 {
                        let b = it.next().unwrap();
                        let exp = it.next().unwrap();
                        match **b {
                            Sym(_) => {
                                match eval(exp.clone(), let_env.clone()) {
                                    Ok(r) => {
                                        env_set(&let_env, b.clone(), r);
                                    },
                                    Err(e) => {
                                        return Err(e);
                                    },
                                }
                            },
                            _ => {
                                return err_str("let* with non-symbol binding");
                            },
                        }
                    }
                },
                _ => return err_str("let* with non-list bindings"),
            }
            ast = a2;
            env = let_env.clone();
            continue 'tco;
        },
        "quote" => {
            return Ok((*args)[1].clone());
        },
        "quasiquote" => {
            let a1 = (*args)[1].clone();
            ast = quasiquote(a1);
            continue 'tco;
        },
        "defmacro!" => {
            let a1 = (*args)[1].clone();
            let a2 = (*args)[2].clone();
            match eval(a2, env.clone()) {
                Ok(r) => {
                    match *r {
                        MalFunc(ref mfd,_) => {
                            match *a1 {
                                Sym(_) => {
                                    let mut new_mfd = mfd.clone();
                                    new_mfd.is_macro = true;
                                    let mf = malfuncd(new_mfd,_nil());
                                    env_set(&env.clone(), a1.clone(), mf.clone());
                                    return Ok(mf);
                                },
                                _ => return err_str("def! of non-symbol"),
                            }
                        },
                        _ => return err_str("def! of non-symbol"),
                    }
                },
                Err(e) => return Err(e),
            }
        },
        "macroexpand" => {
            let a1 = (*args)[1].clone();
            return macroexpand(a1, env.clone())
        },
        "try*" => {
            let a1 = (*args)[1].clone();
            match eval(a1, env.clone()) {
                Ok(res) => return Ok(res),
                Err(err) => {
                    if args.len() < 3 { return Err(err); }
                    let a2 = (*args)[2].clone();
                    let cat = match *a2 {
                        List(ref cat,_) => cat,
                        _ => return err_str("invalid catch* clause"),
                    };
                    if cat.len() != 3 {
                        return err_str("wrong arity to catch* clause");
                    }
                    let c1 = (*cat)[1].clone();
                    match *c1 {
                        Sym(_) => {},
                        _ => return err_str("invalid catch* binding"),
                    };
                    let exc = match err {
                        ErrMalVal(mv) => mv,
                        ErrString(s) => string(s),
                    };
                    let bind_env = env_new(Some(env.clone()));
                    env_set(&bind_env, c1.clone(), exc);
                    let c2 = (*cat)[2].clone();
                    return eval(c2, bind_env);
                },
            };
        }
        "do" => {
            let el = list(args.slice(1,args.len()-1).to_vec());
            match eval_ast(el, env.clone()) {
                Err(e) => return Err(e),
                Ok(_) => {
                    let ref last = args[args.len()-1];
                    ast = last.clone();
                    continue 'tco;
                },
            }
        },
        "if" => {
            let a1 = (*args)[1].clone();
            let cond = eval(a1, env.clone());
            match cond {
                Err(e) => return Err(e),
                Ok(c) => match *c {
                    False | Nil => {
                        if args.len() >= 4 {
                            let a3 = (*args)[3].clone();
                            ast = a3;
                            env = env.clone();
                            continue 'tco;
                        } else {
                            return Ok(_nil());
                        }
                    },
                    _ => {
                        let a2 = (*args)[2].clone();
                        ast = a2;
                        env = env.clone();
                        continue 'tco;
                    },
                }
            }
        },
        "fn*" => {
            let a1 = (*args)[1].clone();
            let a2 = (*args)[2].clone();
            return Ok(malfunc(eval, a2, env.clone(), a1, _nil()));
        },
        "eval" => {
            let a1 = (*args)[1].clone();
            match eval(a1, env.clone()) {
                Ok(exp) => {
                    ast = exp;
                    env = env_root(&env);
                    continue 'tco;
                },
                Err(e) => return Err(e),
            }
        },
        _ => { // function call
            return match eval_ast(ast3, env.clone()) {
                Err(e) => Err(e),
                Ok(el) => {
                    let args = match *el {
                        List(ref args,_) => args,
                        _ => return err_str("Invalid apply"),
                    };
                    match *args.clone()[0] {
                        Func(f,_) => f(args.slice(1,args.len()).to_vec()),
                        MalFunc(ref mf,_) => {
                            let mfc = mf.clone();
                            let alst = list(args.slice(1,args.len()).to_vec());
                            let new_env = env_new(Some(mfc.env.clone()));
                            match env_bind(&new_env, mfc.params, alst) {
                                Ok(_) => {
                                    ast = mfc.exp;
                                    env = new_env;
                                    continue 'tco;
                                },
                                Err(e) => err_str(e.as_slice()),
                            }
                        },
                        _ => err_str("attempt to call non-function"),
                    }
                }
            }
        },
    }

    }
}

// print
fn print(exp: MalVal) -> String {
    exp.pr_str(true)
}

fn rep(str: &str, env: Env) -> Result<String,MalError> {
    match read(str.to_string()) {
        Err(e) => Err(e),
        Ok(ast) => {
            //println!("read: {}", ast);
            match eval(ast, env) {
                Err(e)  => Err(e),
                Ok(exp) => Ok(print(exp)),
            }
        }
    }
}

fn main() {
    // core.rs: defined using rust
    let repl_env = env_new(None);
    for (k, v) in core::ns().into_iter() {
        env_set(&repl_env, symbol(k.as_slice()), v);
    }
    // see eval() for definition of "eval"
    env_set(&repl_env, symbol("*ARGV*".as_slice()), list(vec![]));

    // core.mal: defined using the language itself
    let _ = rep("(def! *host-language* \"rust\")", repl_env.clone());
    let _ = rep("(def! not (fn* (a) (if a false true)))", repl_env.clone());
    let _ = rep("(def! load-file (fn* (f) (eval (read-string (str \"(do \" (slurp f) \")\")))))", repl_env.clone());
    let _ = rep("(defmacro! cond (fn* (& xs) (if (> (count xs) 0) (list 'if (first xs) (if (> (count xs) 1) (nth xs 1) (throw \"odd number of forms to cond\")) (cons 'cond (rest (rest xs)))))))", repl_env.clone());
    let _ = rep("(defmacro! or (fn* (& xs) (if (empty? xs) nil (if (= 1 (count xs)) (first xs) `(let* (or_FIXME ~(first xs)) (if or_FIXME or_FIXME (or ~@(rest xs))))))))", repl_env.clone());

    // Invoked with command line arguments
    let args = os::args();
    if args.len() > 1 {
        let mv_args = args.slice(2,args.len()).iter()
            .map(|a| string(a.to_string()))
            .collect::<Vec<MalVal>>();
        env_set(&repl_env, symbol("*ARGV*".as_slice()), list(mv_args));
        let lf = "(load-file \"".to_string() + args[1] + "\")".to_string();
        match rep(lf.as_slice(), repl_env.clone()) {
            Ok(_) => {
                os::set_exit_status(0);
                return;
            },
            Err(str) => {
                println!("Error: {}", str);
                os::set_exit_status(1);
                return;
            },
        }
    }

    // repl loop
    let _  = rep("(println (str \"Mal [\" *host-language* \"]\"))", repl_env.clone());
    loop {
        let line = readline::mal_readline("user> ");
        match line { None => break, _ => () }
        match rep(line.unwrap().as_slice(), repl_env.clone()) {
            Ok(str)  => println!("{}", str),
            Err(ErrMalVal(_)) => (),  // Blank line
            Err(ErrString(s)) => println!("Error: {}", s),
        }
    }
}
