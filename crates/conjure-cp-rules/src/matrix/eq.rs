use conjure_cp::ast::AbstractLiteral;
use conjure_cp::ast::Atom;
use conjure_cp::into_matrix_expr;
use std::vec;
use conjure_cp::ast::Literal;
use conjure_cp::ast::Metadata;
use conjure_cp::ast::{Expression, Moo, SymbolTable};
use conjure_cp::rule_engine::{
    ApplicationError::RuleNotApplicable, ApplicationResult, Reduction,
    register_rule,
};

/// Converts an eq operation involving two matrices into a bunch of =
#[register_rule(("Base", 8002))]
fn eq_to_compr(expr: &Expression, symtab: &SymbolTable) -> ApplicationResult {
    let mut res: Vec<Expression> = Vec::new();
    if let Expression::Eq(_, a, b) = expr {
   
        if let Expression::Atomic(_, Atom::Reference(d)) = &**a {
                 


            match &**b {

                Expression::AbstractLiteral(_,AbstractLiteral::Matrix(items, _) ) => {

                    let first_dim = items.len();
                    let mut second_dim = 0;

                    if let Expression::AbstractLiteral(_, AbstractLiteral::Matrix(inner, _)) = &items[0] {
                        second_dim = inner.len();
                    }
                    
                    for i in 1..=first_dim {
                        for j in 1..=second_dim {

                            let first_index = Expression::Atomic(Metadata::new(), Atom::Literal(Literal::Int(i.try_into().unwrap())));

                            let second_index = Expression::Atomic(Metadata::new(), Atom::Literal(Literal::Int(j.try_into().unwrap())));

                            let left_side = if let Expression::AbstractLiteral(_, AbstractLiteral::Matrix(inner, _)) = &items[i-1] {
                                inner[j-1].clone()
                            } else {
                                panic!("Expected matrix row at items[{}]", i);
                            };

                            // let left_side = Expression::Atomic(Metadata::new(), Atom::Literal(Literal::Int(left_side_number.try_into().unwrap())));

                            let cur = Expression::Eq(Metadata::new(), Moo::new(Expression::SafeIndex(Metadata::new(), a.clone(), vec![first_index,second_index])), Moo::new(left_side));
                            
                            // println!("{:?}",cur);
                            res.push(cur);
                        }
                    }
                    let res_expr: Expression = into_matrix_expr!(res);
                    println!("qweqwe");
                    return Ok(Reduction::pure(res_expr));
//  return Err(RuleNotApplicable);
                }


                Expression::Atomic(_, Atom::Literal(Literal::AbstractLiteral(AbstractLiteral::Matrix(items, _))))  => {

                

                let first_dim = items.len();
                println!("{}",first_dim);
                let mut second_dim = 0;

                if let Literal::AbstractLiteral(AbstractLiteral::Matrix(inner, _)) = &items[0] {
                    second_dim = inner.len();
                }
                
                for i in 1..=first_dim {
                    for j in 1..=second_dim {

                        let first_index = Expression::Atomic(Metadata::new(), Atom::Literal(Literal::Int(i.try_into().unwrap())));

                        let second_index = Expression::Atomic(Metadata::new(), Atom::Literal(Literal::Int(j.try_into().unwrap())));

                        let left_side_number = if let Literal::AbstractLiteral(AbstractLiteral::Matrix(inner, _)) = &items[i-1] {
                            inner[j-1].clone()
                        } else {
                            panic!("Expected matrix row at items[{}]", i);
                        };

                        let left_side = Expression::Atomic(Metadata::new(), Atom::Literal(Literal::Int(left_side_number.try_into().unwrap())));

                        let cur = Expression::Eq(Metadata::new(), Moo::new(Expression::SafeIndex(Metadata::new(), a.clone(), vec![first_index,second_index])), Moo::new(left_side));
                         
                        // println!("{:?}",cur);
                        res.push(cur);
                    }
                }
                let res_expr: Expression = into_matrix_expr!(res);
                return Ok(Reduction::pure(res_expr));


            },
            _ =>  {return Err(RuleNotApplicable);}
        }
                
    }
}
        
    return Err(RuleNotApplicable);
    
}








