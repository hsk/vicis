use crate::ir::{
    function::parser::ParserContext,
    module::name,
    types::{self, Type, Types, I1, I32, I64, I8},
    util::{spaces, string_literal},
    value::{
        ConstantArray, ConstantData, ConstantExpr, ConstantInt, ConstantStruct, Value, ValueId,
    },
};
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{char, digit1},
    combinator::{opt, recognize},
    error::VerboseError,
    sequence::{preceded, tuple},
    IResult,
};

pub fn parse_constant<'a>(
    source: &'a str,
    types: &Types,
    ty: Type,
) -> IResult<&'a str, ConstantData, VerboseError<&'a str>> {
    if let Ok((source, _)) = preceded(spaces, tag("undef"))(source) {
        return Ok((source, ConstantData::Undef));
    }
    if let Ok((source, _)) = preceded(spaces, tag("null"))(source) {
        return Ok((source, ConstantData::Null));
    }
    if let Ok((source, _)) = preceded(spaces, tag("zeroinitializer"))(source) {
        return Ok((source, ConstantData::AggregateZero));
    }
    if let Ok((source, id)) = parse_constant_int(source, ty) {
        return Ok((source, id.into()));
    }
    if let Ok((source, id)) = parse_constant_array(source, types) {
        return Ok((source, id));
    }
    if let Ok((source, id)) = parse_constant_global_ref(source) {
        return Ok((source, id));
    }
    if let Ok((source, id)) = parse_constant_struct(source, types) {
        return Ok((source, id));
    }
    parse_constant_expr(source, types)
}

pub fn parse_constant_int<'a>(
    source: &'a str,
    ty: Type,
) -> IResult<&'a str, ConstantInt, VerboseError<&'a str>> {
    let (source, num) = preceded(
        spaces,
        recognize(tuple((
            opt(char('-')),
            alt((digit1, tag("true"), tag("false"))),
        ))),
    )(source)?;
    let val = match ty {
        I1 => ConstantInt::Int1(num == "true"),
        I8 => ConstantInt::Int8(num.parse::<i8>().unwrap()),
        I32 => ConstantInt::Int32(num.parse::<i32>().unwrap()),
        I64 => ConstantInt::Int64(num.parse::<i64>().unwrap()),
        _ => todo!(),
    };
    Ok((source, val))
}

pub fn parse_constant_array<'a>(
    source: &'a str,
    _types: &Types,
    // ty: Type,
) -> IResult<&'a str, ConstantData, VerboseError<&'a str>> {
    // TODO: Support arrays in the form of [a, b, c]
    let (source, _) = preceded(spaces, char('c'))(source)?;
    let (source, s) = preceded(spaces, string_literal)(source)?;
    let val = ConstantData::Array(ConstantArray {
        elem_ty: I8,
        elems: s
            .as_bytes()
            .iter()
            .map(|c| ConstantData::Int(ConstantInt::Int8(*c as i8)))
            .collect(),
        is_string: true,
    });
    Ok((source, val))

    // let (mut source, _) = preceded(spaces, char('['))(source)?;
    // loop {
    //     let (source_, ty) = types::parse(source, ctx.types)?;
    //
    // }

    // let (source, num) = preceded(spaces, digit1)(source)?;
    // let val = match &*ctx.types.get(ty) {
    //     Type::Int(32) => Value::Constant(ConstantData::Int(ConstantInt::Int32(
    //         num.parse::<i32>().unwrap(),
    //     ))),
    //     _ => todo!(),
    // };
    // Ok((source, ctx.data.create_value(val)))
}

pub fn parse_constant_expr<'a>(
    source: &'a str,
    types: &Types,
) -> IResult<&'a str, ConstantData, VerboseError<&'a str>> {
    if let Ok((source, konst)) = parse_constant_getelementptr(source, types) {
        return Ok((source, konst));
    }
    parse_constant_bitcast(source, types)
}

pub fn parse_constant_getelementptr<'a>(
    source: &'a str,
    types: &Types,
) -> IResult<&'a str, ConstantData, VerboseError<&'a str>> {
    let (source, _) = preceded(spaces, tag("getelementptr"))(source)?;
    let (source, inbounds) = opt(preceded(spaces, tag("inbounds")))(source)?;
    let (source, _) = preceded(spaces, char('('))(source)?;
    let (source, ty) = types::parse(source, types)?;
    let (mut source, _) = preceded(spaces, char(','))(source)?;
    let mut args = vec![];
    let mut tys = vec![ty];
    loop {
        let (source_, ty) = types::parse(source, types)?;
        let (source_, arg) = parse_constant(source_, types, ty)?;
        tys.push(ty);
        args.push(arg);
        if let Ok((source_, _)) = preceded(spaces, char(','))(source_) {
            source = source_;
            continue;
        }
        if let Ok((source, _)) = preceded(spaces, char(')'))(source_) {
            return Ok((
                source,
                ConstantData::Expr(ConstantExpr::GetElementPtr {
                    inbounds: inbounds.is_some(),
                    tys,
                    args,
                }),
            ));
        }
    }
}

pub fn parse_constant_bitcast<'a>(
    source: &'a str,
    types: &Types,
) -> IResult<&'a str, ConstantData, VerboseError<&'a str>> {
    let (source, _) = preceded(spaces, tag("bitcast"))(source)?;
    let (source, _) = preceded(spaces, char('('))(source)?;
    let (source, from) = types::parse(source, types)?;
    let (source, arg) = parse_constant(source, types, from)?;
    let (source, _) = preceded(spaces, tag("to"))(source)?;
    let (source, to) = types::parse(source, types)?;
    let (source, _) = preceded(spaces, char(')'))(source)?;
    Ok((
        source,
        ConstantData::Expr(ConstantExpr::Bitcast {
            tys: [from, to],
            arg: Box::new(arg),
        }),
    ))
}

pub fn parse_constant_global_ref(source: &str) -> IResult<&str, ConstantData, VerboseError<&str>> {
    let (source, name) = preceded(spaces, preceded(char('@'), name::parse))(source)?;
    Ok((source, ConstantData::GlobalRef(name)))
}

pub fn parse_constant_struct<'a>(
    source: &'a str,
    types: &Types,
) -> IResult<&'a str, ConstantData, VerboseError<&'a str>> {
    let (mut source, is_packed) = preceded(spaces, alt((tag("{"), tag("<{"))))(source)?;
    let is_packed = is_packed == "<{";
    let mut elems = vec![];
    let mut elems_ty = vec![];
    loop {
        let (source_, t) = types::parse(source, types)?;
        let (source_, konst) = parse_constant(source_, types, t)?;
        elems.push(konst);
        elems_ty.push(t);
        if let Ok((source_, _)) = preceded(spaces, char(','))(source_) {
            source = source_;
            continue;
        }
        let (source_, _) = preceded(spaces, tag(if is_packed { "}>" } else { "}" }))(source_)?;
        return Ok((
            source_,
            ConstantData::Struct(ConstantStruct {
                elems_ty,
                elems,
                is_packed,
            }),
        ));
    }
}

pub fn parse_local<'a, 'b>(
    source: &'a str,
    ctx: &mut ParserContext<'b>,
    _ty: Type,
) -> IResult<&'a str, ValueId, VerboseError<&'a str>> {
    let (source, name) = preceded(spaces, preceded(char('%'), name::parse))(source)?;
    Ok((source, ctx.get_or_create_named_value(name)))
}

pub fn parse<'a, 'b>(
    source: &'a str,
    ctx: &mut ParserContext<'b>,
    ty: Type,
) -> IResult<&'a str, ValueId, VerboseError<&'a str>> {
    if let Ok((source, konst)) = parse_constant(source, ctx.types, ty) {
        let id = ctx.data.create_value(Value::Constant(konst));
        return Ok((source, id));
    }

    parse_local(source, ctx, ty)
}
