pub enum PlannerAction {
    TicketCreation(f64),
    Wait,
}

pub fn planner(confidence: &f64) -> PlannerAction {
    if *confidence > 0.65 {
        PlannerAction::TicketCreation(*confidence)
    } else {
        PlannerAction::Wait
    }
}
