pub mod agents;
pub mod registry;
pub mod users;
pub mod web;
pub mod tournament_mananger {
    tonic::include_proto!("achtung.tournament");
}
