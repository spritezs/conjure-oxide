//! conjure_oxide solve sub-command
#![allow(clippy::unwrap_used)]
use std::{
    fmt::Pointer, fs::{File, OpenOptions}, io::{self, BufRead, Write as _}, path::PathBuf, process::exit, sync::{Arc, RwLock}
};
use std::fs;
use conjure_cp::ast::{Atom, DeclarationPtr, Expression, Metadata, Moo};
use std::collections::BTreeMap;
use conjure_cp::solver::adaptors::{Minion, Smt, Sat};
use conjure_cp::ast::{Literal, Name};
use std::collections::HashMap;
use anyhow::{anyhow, ensure};
use clap::ValueHint;
use conjure_cp::defaults::DEFAULT_RULE_SETS;
use conjure_cp::parse::tree_sitter::parse_essence_file_native;
use conjure_cp::{
    Model,
    ast::comprehension::USE_OPTIMISED_REWRITER_FOR_COMPREHENSIONS,
    context::Context,
    rule_engine::{resolve_rule_sets, rewrite_morph, rewrite_naive},
    solver::{Solver, adaptors},
};
use conjure_cp::{
    parse::conjure_json::model_from_json, rule_engine::get_rules, solver::SolverFamily,
};
use conjure_cp_cli::find_conjure::conjure_executable;
use conjure_cp_cli::utils::conjure::{get_solutions_no_dominance, solutions_to_json};
use conjure_cp_cli::utils::json::extract_matrix;
use serde_json::to_string_pretty;

use crate::cli::{GlobalArgs, LOGGING_HELP_HEADING};

#[derive(Clone, Debug, clap::Args)]
pub struct Args {
    /// The input Essence file
    #[arg(value_name = "INPUT_ESSENCE", value_hint = ValueHint::FilePath)]
    pub input_file: PathBuf,

    /// Save execution info as JSON to the given filepath.
    #[arg(long ,value_hint=ValueHint::FilePath,help_heading=LOGGING_HELP_HEADING)]
    pub info_json_path: Option<PathBuf>,

    /// Do not run the solver.
    ///
    /// The rewritten model is printed to stdout in an Essence-style syntax
    /// (but is not necessarily valid Essence).
    #[arg(long, default_value_t = false)]
    pub no_run_solver: bool,

    /// Number of solutions to return. 0 returns all solutions
    #[arg(long, default_value_t = 0, short = 'n')]
    pub number_of_solutions: i32,

    /// Save solutions to the given JSON file
    #[arg(long, short = 'o', value_hint = ValueHint::FilePath,help_heading=LOGGING_HELP_HEADING)]
    pub output: Option<PathBuf>,
}

pub fn run_solve_command(global_args: GlobalArgs, solve_args: Args) -> anyhow::Result<()> {
    let input_file = solve_args.input_file.clone();

    // each step is in its own method so that similar commands
    // (e.g. testsolve) can reuse some of these steps.

    let context = init_context(&global_args, input_file)?;
    let model = parse(&global_args, Arc::clone(&context))?;

    let rewritten_model = rewrite(model, &global_args, Arc::clone(&context))?;

    if solve_args.no_run_solver {
       

        // TODO: we want to be able to do let solver = match family {....}, but something weird is
        // happening in the types..
        if let Some(path) = global_args.save_solver_input_file {
            eprintln!("Writing solver input file to {}", path.display());
            let mut file = File::create(path).unwrap();

            match global_args.solver {
                SolverFamily::Sat => {
                    let solver = Solver::new(adaptors::Sat::default());
                    let solver = solver.load_model(rewritten_model)?;
                    solver.write_solver_input_file(&mut file)?;
                }
                SolverFamily::Smt => {
                    let solver = Solver::new(adaptors::Smt::default());
                    let solver = solver.load_model(rewritten_model)?;
                    solver.write_solver_input_file(&mut file)?;
                }
                SolverFamily::Minion => {
                    let solver = Solver::new(adaptors::Minion::default());
                    let solver = solver.load_model(rewritten_model)?;
                    solver.write_solver_input_file(&mut file)?;
                }
            };
        }
    } else {
        run_solver(
            global_args.solver,
            &global_args,
            &solve_args,
            rewritten_model,
        )?
    }

    // still do postamble even if we didn't run the solver
    if let Some(ref path) = solve_args.info_json_path {
        let context_obj = context.read().unwrap().clone();
        let generated_json = &serde_json::to_value(context_obj)?;
        let pretty_json = serde_json::to_string_pretty(&generated_json)?;
        File::create(path)?.write_all(pretty_json.as_bytes())?;
    }
    Ok(())
}

/// Initialises the context for solving.
pub(crate) fn init_context(
    global_args: &GlobalArgs,
    input_file: PathBuf,
) -> anyhow::Result<Arc<RwLock<Context<'static>>>> {
    let target_family = global_args.solver;
    let mut extra_rule_sets: Vec<&str> = DEFAULT_RULE_SETS.to_vec();
    for rs in &global_args.extra_rule_sets {
        extra_rule_sets.push(rs.as_str());
    }

    if global_args.no_use_expand_ac {
        extra_rule_sets.pop_if(|x| x == &"Better_AC_Comprehension_Expansion");
    }

    let rule_sets = match resolve_rule_sets(target_family, &extra_rule_sets) {
        Ok(rs) => rs,
        Err(e) => {
            tracing::error!("Error resolving rule sets: {}", e);
            exit(1);
        }
    };

    let pretty_rule_sets = rule_sets
        .iter()
        .map(|rule_set| rule_set.name)
        .collect::<Vec<_>>()
        .join(", ");

    tracing::info!("Enabled rule sets: [{}]", pretty_rule_sets);
    tracing::info!(
        target: "file",
        "Rule sets: {}",
        pretty_rule_sets
    );

    let rules = get_rules(&rule_sets)?.into_iter().collect::<Vec<_>>();
    tracing::info!(
        target: "file",
        "Rules: {}",
        rules.iter().map(|rd| format!("{rd}")).collect::<Vec<_>>().join("\n")
    );
    let context = Context::new_ptr(
        target_family,
        extra_rule_sets.iter().map(|rs| rs.to_string()).collect(),
        rules,
        rule_sets.clone(),
    );

    context.write().unwrap().file_name = Some(input_file.to_str().expect("").into());

    Ok(context)
}

pub(crate) fn parse(
    global_args: &GlobalArgs,
    context: Arc<RwLock<Context<'static>>>,
) -> anyhow::Result<Model> {
    let input_file: String = context
        .read()
        .unwrap()
        .file_name
        .clone()
        .expect("context should contain the input file");

    tracing::info!(target: "file", "Input file: {}", input_file);
    if global_args.use_native_parser {
        parse_essence_file_native(input_file.as_str(), context.clone()).map_err(|e| e.into())
    } else {
        conjure_executable()
            .map_err(|e| anyhow!("Could not find correct conjure executable: {e}"))?;

        let mut cmd = std::process::Command::new("conjure");
        let output = cmd
            .arg("pretty")
            .arg("--output-format=astjson")
            .arg(input_file)
            .output()?;

        let conjure_stderr = String::from_utf8(output.stderr)?;

        if !conjure_stderr.is_empty() {
            println!("{}",conjure_stderr);
        }
        ensure!(conjure_stderr.is_empty(), conjure_stderr);

        let astjson = String::from_utf8(output.stdout)?;

        if cfg!(feature = "extra-rule-checks") {
            tracing::info!("extra-rule-checks: enabled");
        } else {
            tracing::info!("extra-rule-checks: disabled");
        }

        model_from_json(&astjson, context.clone()).map_err(|e| anyhow!(e))
    }
}

pub(crate) fn rewrite(
    model: Model,
    global_args: &GlobalArgs,
    context: Arc<RwLock<Context<'static>>>,
) -> anyhow::Result<Model> {
    tracing::info!("Initial model: \n{}\n", model);

    let rule_sets = context.read().unwrap().rule_sets.clone();

    let new_model = if global_args.use_optimised_rewriter {
        USE_OPTIMISED_REWRITER_FOR_COMPREHENSIONS.store(true, std::sync::atomic::Ordering::Relaxed);
        tracing::info!("Rewriting the model using the optimising rewriter");
        rewrite_morph(
            model,
            &rule_sets,
            global_args.check_equally_applicable_rules,
        )
    } else {
        tracing::info!("Rewriting the model using the default / naive rewriter");
        if global_args.exit_after_unrolling {
            tracing::info!("Exiting after unrolling");
        }
        rewrite_naive(
            &model,
            &rule_sets,
            global_args.check_equally_applicable_rules,
            global_args.exit_after_unrolling,
        )?
    };

    tracing::info!("Rewritten model: \n{}\n", new_model);
    Ok(new_model)
}

fn run_solver(
    solver: SolverFamily,
    global_args: &GlobalArgs,
    cmd_args: &Args,
    model: Model,
) -> anyhow::Result<()> {
    let out_file: Option<File> = match &cmd_args.output {
        None => None,
        Some(pth) => Some(
            File::options()
                .create(true)
                .truncate(true)
                .write(true)
                .open(pth)?,
        ),
    };

   
    let dom_file = "rel_dom.essence";
    let incomp_file = "incomp_fct.essence";

    let solutions: Vec<BTreeMap<Name, Literal>>;
    if let Some(parent_dir) = cmd_args.input_file.parent() {

        let dom_file_path = parent_dir.join(dom_file);
        if dom_file_path.exists() {
            println!("Dom Rel file '{}' found in the same directory as input file!", dom_file);

            let file = File::open(&dom_file_path)?;
            let reader = io::BufReader::new(file);

            let mut lines_to_write = Vec::new();
            let mut found_such_that = false;

            let file2 = File::open(cmd_args.input_file.clone())?;
            let reader2 = io::BufReader::new(file2);


            for line in reader2.lines() {
                let line = line?; 
                
                if line.contains("such that") {
                    break;
                }
                lines_to_write.push(line);
            }

            for line in reader.lines() {
                let line = line?; 
                
                if found_such_that || line.contains("such that") {
                    found_such_that = true;
                    lines_to_write.push(line);
                }
            }

            let mut file = OpenOptions::new().write(true).truncate(true).open(&dom_file_path)?;
            for line in lines_to_write {
                writeln!(file, "{}", line)?;
            }
            let incomp_fct_path = parent_dir.join(incomp_file);
            let mut total_time: f64 = 0.0;
            if incomp_fct_path.exists() {
                solutions = get_solutions_with_incomparability(solver, model, dom_file_path, &global_args, &mut total_time, incomp_fct_path)?;
            }
            else
            {
                solutions = get_solutions_with_dominance(solver, model.clone(), dom_file_path.clone(), &global_args, &mut total_time)?;
            }
            
            match &cmd_args.output {
                None => {
                    return Err(anyhow::anyhow!("Output path is None").context("Expected an output file path"));
                 },
                Some(pth) => {
                    let mut new_path = pth.clone();
                    if let Some(parent_dir) = new_path.parent() {
                        let new_file_name = "time.json";
                        let new_full_path = parent_dir.join(new_file_name);
                        new_path = new_full_path;
                    }
                    File::create(new_path)?.write_all(format!("Total time: {}\n", total_time).as_bytes())?; 
                }
            };
            
        } else {
            match solver {
            SolverFamily::Sat => {
                 solutions = get_solutions_no_dominance(Sat::default(), model, cmd_args.number_of_solutions, &global_args.save_solver_input_file, None)?
            }
            SolverFamily::Minion => {
                 solutions = get_solutions_no_dominance(Minion::default(), model, cmd_args.number_of_solutions, &global_args.save_solver_input_file, None)?
            }
            SolverFamily::Smt => {
                 solutions = get_solutions_no_dominance(Smt::default(), model, cmd_args.number_of_solutions, &global_args.save_solver_input_file, None)?
            }
        }
        }
    } else {
        return Err(anyhow::anyhow!("Input path does not have a parent directory").into());
    }

    tracing::info!(target: "file", "Solutions: {}", solutions_to_json(&solutions));

    let solutions_json = solutions_to_json(&solutions);
    let solutions_str = to_string_pretty(&solutions_json)?;
    match out_file {
        None => {
            println!("Solutions:");
            println!("{solutions_str}");
        }
        Some(mut outf) => {
            outf.write_all(solutions_str.as_bytes())?;
            println!(
                "Solutions saved to {:?}",
                &cmd_args.output.clone().unwrap().canonicalize()?
            )
        }
    }
    Ok(())
}


pub fn get_solutions_with_dominance(
    solver: SolverFamily,
    mut model: Model,
    dom_file_path: PathBuf,
    global_args: &GlobalArgs,
    total_time: &mut f64,
) -> Result<Vec<BTreeMap<Name, Literal>>, anyhow::Error> {
    // all non-dominated solutions
    let mut results = Vec::new();
    let mut sols_to_constraints = HashMap::new();
    loop {
        // get the next solution
        let solutions = match solver {
            SolverFamily::Sat => {
                get_solutions_no_dominance(Sat::default(), model.clone(), 1, &global_args.save_solver_input_file, Some(total_time))?
            }
            SolverFamily::Minion => {
                get_solutions_no_dominance(Minion::default(), model.clone(), 1, &global_args.save_solver_input_file, Some(total_time))?
            }
            SolverFamily::Smt => {
                get_solutions_no_dominance(Smt::default(), model.clone(), 1, &global_args.save_solver_input_file, Some(total_time))?
            }
        };
        // no more solutions
        let Some(solution) = solutions.first() else {
            break;
        };
        // add to results
        results.extend(solutions.clone());

        let blocking_constraints =
            crate_blocking_constraint_from_solution(solution, dom_file_path.clone(), &global_args)
            .ok_or_else(|| anyhow::anyhow!(
                "Failed to generate blocking constraints for solution: {:?}", 
                solution
            ))?;

        sols_to_constraints.insert(solution.clone(), blocking_constraints.clone());
        
        // create and apply new blocking constraints
        model.add_constraints(blocking_constraints);
    }

    Ok(results)
}


pub fn get_solutions_with_incomparability(
    solver: SolverFamily,
    mut model: Model,
    dom_file_path: PathBuf,
    global_args: &GlobalArgs,
    total_time: &mut f64,
    incomp_file_path: PathBuf,
) -> Result<Vec<BTreeMap<Name, Literal>>, anyhow::Error> {
    // all non-dominated solutions
    let mut results = Vec::new();
    let mut sols_to_constraints = HashMap::new();

    let incomp_text = fs::read_to_string(&incomp_file_path)
        .expect(&format!("Failed to read incomp file: {}", incomp_file_path.display()));

   let start = incomp_text
        .find('(')
        .ok_or_else(|| anyhow::anyhow!("missing '(' in {}", incomp_text))?
        + 1;

    let end = incomp_text
        .find(')')
        .ok_or_else(|| anyhow::anyhow!("missing ')' in {}", incomp_text))?;

    let incomp_var_name = &incomp_text[start..end];
    let ordering = &incomp_text[..start - 1];

    let incomp_var = model.get_var(&Name::from(incomp_var_name)).unwrap();
    let levels = incomp_var.domain().ok_or_else(|| anyhow::anyhow!("Couldn't calculate levels"))?;
    let mut level_values = levels.values_i32()?;

    if ordering=="descending"{
        level_values.reverse();
    }
    loop {
        for level in level_values {
            println!("level is {}",level);
            let incomp_var = model.get_var(&Name::from("s")).unwrap();

            // create level constraint
            let level_constraint = crate_level_constraint_from_incomp_fct(&model, incomp_var, &level);
            model.add_constraints(level_constraint.clone());            

            // get every solutions for this level
            let solutions = match solver {
                SolverFamily::Sat => {
                    get_solutions_no_dominance(Sat::default(), model.clone(), -1, &global_args.save_solver_input_file, Some(total_time))?
                }
                SolverFamily::Minion => {
                    get_solutions_no_dominance(Minion::default(), model.clone(), -1, &global_args.save_solver_input_file, Some(total_time))?
                }
                SolverFamily::Smt => {
                    get_solutions_no_dominance(Smt::default(), model.clone(), -1, &global_args.save_solver_input_file, Some(total_time))?
                }
            };
            
            // add to results
            results.extend(solutions.clone());

            let mut new_constraints = Vec::new();

            for solution in &solutions{
                    let blocking_constraints =
                crate_blocking_constraint_from_solution(solution, dom_file_path.clone(), &global_args)
                .ok_or_else(|| anyhow::anyhow!(
                    "Failed to generate blocking constraints for solution: {:?}", 
                    solution
                ))?;
                
                new_constraints.extend(blocking_constraints);
            }
            for solution in solutions {
                sols_to_constraints.insert(solution.clone(), new_constraints.clone());
            }
            
            // create and apply new blocking constraints
            model.add_constraints(new_constraints);
            model.remove_constraints(level_constraint);
        }
        return Ok(results);
    }

}

pub fn crate_blocking_constraint_from_solution(
    solution: &BTreeMap<Name, Literal>,
    dom_file_path: PathBuf,
    global_args: &GlobalArgs,
) -> Option<Vec<Expression>> {


    // read domrel model
    let file_content = fs::read_to_string(&dom_file_path)
        .expect(&format!("Failed to read dom file: {}", dom_file_path.display()));

    
    let mut modified_content = file_content;

    // sub in solution
    for (var, value) in solution {
        match value {
            Literal::Int(i) => {
                let replacement = format!("{}", i);
                modified_content = modified_content.replace(&format!("fromSol({})", var), &replacement);
            },
            Literal::Bool(b) => {
                let replacement = format!("{}", b);
                modified_content = modified_content.replace(&format!("fromSol({})", var), &replacement);
            },
            _ => {
                modified_content = modified_content.replace(&format!("fromSol({})", var), &extract_matrix(value));
            },
        }
    }

    // write model to new file
    let output_file_path = generate_output_file_path(&dom_file_path);
    let _ = fs::write(&output_file_path, modified_content);

    // parse model
    let context = init_context(&global_args, output_file_path).ok()?;

    let model = parse(&global_args, Arc::clone(&context)).ok()?;

    let rewritten = rewrite(model, &global_args, Arc::clone(&context)).ok()?;
    // add constraints to model

    Some(rewritten
        .as_submodel()
        .constraints()
        .clone())
}


fn generate_output_file_path(dom_file_path: &PathBuf) -> PathBuf {
    let output_dir = dom_file_path.parent().unwrap();
    let new_file_name = format!("modified_{}", dom_file_path.file_name().unwrap().to_str().unwrap());
    
    output_dir.join(new_file_name)
}

pub fn crate_level_constraint_from_incomp_fct(
    model: &Model,
    name: DeclarationPtr,
    level: &i32
) -> Vec<Expression> {
    let new_level_blocking = Expression::Eq(Metadata::new(), Moo::new(Expression::Atomic(Metadata::new(), Atom::Reference(name))), Moo::new(Expression::Atomic(Metadata::new(), Atom::from(*level))));

    let mut model_copy = model.clone();
    model_copy.remove_constraints(model_copy.as_submodel().constraints().clone());
    model_copy.add_constraint(new_level_blocking);

    // rewrite model
    let rule_sets = model.context.read().unwrap().rule_sets.clone();

    let rewritten = rewrite_naive(&model_copy, &rule_sets, false, false);

    rewritten
        .expect("Should be able to rewrite the model")
        .as_submodel()
        .constraints()
        .clone()
}

