require! LiveScript

require! 'prelude-ls': {map}
require! './builtins.ls': {truthy, is-seq}
require! './printer.ls': {pr-str}
require! './env': {create-env, bind-value}

{Lambda, MalList, MalVec, MalMap} = require './types.ls'

SPECIALS = <[ do let def! fn* fn defn if ]>

export eval-mal = (env, ast) -->
    if is-special-form ast
        return handle-special-form env, ast
    ev = eval-mal env

    evaluated = switch ast.type
        | \SYM => env[ast.value] or throw new Error "Undefined symbol: #{ ast.value }"
        | \LIST => new MalList map ev, ast.value
        | \VEC => new MalVec map ev, ast.value
        | \MAP => new MalMap [[(ev k), (ev ast.get(k))] for k in ast.keys()]
        | otherwise => ast
    if evaluated.type is \LIST
        apply-ast env, evaluated.value
    else
        evaluated

is-special-form = (ast) ->
    | ast.type is \LIST and ast.value.length > 0 =>
        [{type, value}] = ast.value
        (type is \SYM) and (value in SPECIALS)
    | otherwise => false

handle-special-form = (env, {value: [form, ...args]}) ->
    switch form.value
        | 'do' => do-do env, args
        | 'let' => do-let env, args
        | 'def!', 'def' => do-def env, args
        | 'fn', 'fn*' => do-fn env, args
        | 'defn' => do-def env, [args[0], (do-fn env, args.slice(1))]
        | 'if' => do-if env, args
        | _ => throw new Error "Unknown form: #{ form.value }"

do-fn = (env, [names, ...bodies]) ->
    unless is-seq names
        throw new Error "Names must be a sequence, got: #{ pr-str names }"
    new Lambda env, names.value.slice(), bodies

do-if = (env, [test, when-true, when-false]:forms) ->
    unless forms.length is 3
        throw new Error "Expected 3 arguments to if, got #{ forms.length }"
    ret = eval-mal env, test
    eval-mal env, (if (truthy ret) then when-true else when-false)

do-def = (env, [key, value]:forms) ->
    unless forms.length is 2
        throw new Error "Expected 2 arguments to def, got #{ forms.length }"
    bind-value env, key, eval-mal env, value

do-let = (outer, [bindings, ...bodies]) ->
    env = create-env outer
    unless is-seq bindings
        throw new Error "Bindings must be a sequence, got: #{ pr-str bindings }"
    if bindings.value.length % 2
        throw new Error "There must be an even number of bindings"

    for i in [0 til bindings.value.length - 1 by 2]
        do-def env, [bindings.value[i], bindings.value[i + 1]]

    do-do env, bodies

do-do = (env, bodies) ->
    for body in bodies
        ret = eval-mal env, body
    return ret

apply-ast = (env, [fn, ...args]) ->
    switch fn.type
        | \BUILTIN => fn.fn args
        | \LAMBDA => do-do (fn.closure map (eval-mal env), args), fn.body
        | _ => throw new Error "Not a function: #{ pr-str fn }"

