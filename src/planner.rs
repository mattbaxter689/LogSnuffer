pub enum PlannerAction {
    TicketCreation,
    Wait,
}

pub fn planner(confidence: &f64) -> PlannerAction {
    if *confidence > 0.7 {
        PlannerAction::TicketCreation
    } else {
        PlannerAction::Wait
    }
}
