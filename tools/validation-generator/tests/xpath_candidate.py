"""Project the serialized candidate Expr contract; never parse source text."""

from xpath_structure import Unsupported


def adapt(node):
    try:
        return project(node)
    except (KeyError, TypeError) as error:
        raise Unsupported("malformed candidate expression: " + str(error)) from error


def project(x):
    kind = x["kind"]
    if kind in {"root", "context", "any"}:
        return (kind,)
    if kind == "number":
        return (kind, x["lexical_kind"], x["lexeme"])
    if kind == "string":
        return (kind, x["value"])
    if kind == "variable":
        return (kind, x["name"], x["binding"])
    if kind == "bind":
        return (kind, x["operator"], tuple((b["name"], b["id"], adapt(b["value"]))
                                         for b in x["bindings"]), adapt(x["result"]))
    if kind == "step":
        return (kind, x["axis"], adapt(x["test"]))
    if kind == "name":
        return (kind, x["name"])
    if kind == "namespace":
        return (kind, x["prefix"], x["namespace"])
    if kind in {"local", "kind"}:
        return (kind, x["local"] if kind == "local" else x["name"])
    if kind == "call":
        return (kind, x["name"], tuple(map(adapt, x["arguments"])))
    if kind == "binary":
        return (kind, x["operator"], adapt(x["left"]), adapt(x["right"]))
    if kind == "unary":
        return (kind, x["operator"], adapt(x["operand"]))
    if kind == "group":
        return (kind, adapt(x["operand"]))
    if kind == "filter":
        return (kind, adapt(x["base"]), adapt(x["predicate"]))
    if kind == "sequence":
        return (kind, tuple(map(adapt, x["items"])))
    if kind == "if":
        return (kind, adapt(x["condition"]), adapt(x["then"]), adapt(x["otherwise"]))
    if kind == "type_op":
        return ("type", x["operator"], adapt(x["operand"]),
                x["sequence_type"]["name"], x["sequence_type"]["occurrence"])
    raise Unsupported("unsupported candidate node " + str(kind))
