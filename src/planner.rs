pub enum PlannerAction {
    TicketCreation(f64),
    Test,
}

pub fn planner(confidence: &f64) -> PlannerAction {
    if *confidence > 0.5 {
        PlannerAction::TicketCreation(*confidence)
    } else {
        PlannerAction::Test
    }
}
