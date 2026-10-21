pub trait Step {
    fn run(&self);
}

pub struct Show;

impl Step for Show {
    fn run(&self) {
        println!("shown");
    }
}

fn run_dyn(s: &dyn Step) {
    s.run();
}

fn run_generic<S: Step>(s: &S) {
    s.run();
}

fn run_impl(s: &impl Step) {
    s.run();
}

pub fn main_like() {
    run_dyn(&Show);
    run_generic(&Show);
    run_impl(&Show);
}
