use conjure_cp::ast::AbstractLiteral;
use conjure_cp::ast::comprehension::ComprehensionBuilder;
use conjure_cp::ast::Atom;
use conjure_cp::ast::Expression::Atomic;
use conjure_cp::ast::ac_operators::ACOperatorKind;
use std::rc::Rc;
use std::cell::RefCell;
use conjure_cp::ast::Metadata;
use conjure_cp::ast::{Domain, Expression, Moo, SymbolTable};
use conjure_cp::rule_engine::{
    ApplicationError::RuleNotApplicable, ApplicationResult, Reduction,
    register_rule,
};

/// Converts an eq operation involving two matrices into a comprehension
#[register_rule(("Base", 8002))]
fn eq_to_compr(expr: &Expression, symtab: &SymbolTable) -> ApplicationResult {
    
    if let Expression::Eq(_, a, b) = expr {
        if let (Expression::Atomic(_, Atom::Reference(d)), Expression::AbstractLiteral(_, AbstractLiteral::Matrix(items, _))) = (&**a, &**b) {
            let first_dim = items.len();
            let mut second_dim = 0;
            if let Expression::AbstractLiteral(_, AbstractLiteral::Matrix(inner, _)) = &items[0] {
                second_dim = inner.len();
            }

            let first_domain: Vec<i32> = (1..=first_dim as i32).collect();
            let second_domain: Vec<i32> = (1..=second_dim as i32).collect();

            let new_sym = symtab.clone();
            let mut cb = ComprehensionBuilder::new(Rc::new(RefCell::new(new_sym.clone())));
            
            let i = cb.generator_symboltable().borrow_mut().gensym(&Domain::from_slice_i32(&first_domain));
            let j = cb.generator_symboltable().borrow_mut().gensym(&Domain::from_slice_i32(&second_domain));

            cb = cb.generator(i.clone());
            cb = cb.generator(j.clone());



            let atomic_i = Atomic(
                Metadata::new(),
                Atom::Reference(i.clone()),
            );

            let atomic_j = Atomic(
                Metadata::new(),
                Atom::Reference(j.clone()),
            );

            let left_term = Moo::new(Expression::SafeIndex(
                Metadata::new(),
                a.clone(),
                vec![atomic_i.clone(), atomic_j.clone()],
            ));
            
            let right_term = Moo::new(Expression::SafeIndex(
                Metadata::new(),
                b.clone(),
                vec![atomic_i, atomic_j],
            ));
            
            let expr = Moo::new(Expression::Eq(Metadata::new(), left_term, right_term));
            let comprehension = cb.with_return_value(expr.into(), Some(ACOperatorKind::And));



            return Ok(Reduction::with_symbols(Expression::And(Metadata::new(),Moo::new(Expression::Comprehension(
                Metadata::new(),
                Moo::new(comprehension),
            ))),
           new_sym
            ));


        };
}

    
    return Err(RuleNotApplicable);
}