use leptos::*;

use crate::components::achtung_live::AchtungLive;

#[component]
pub fn LiveBattle() -> impl IntoView {
    view! {
        <h1>"Live battle"</h1>
        <AchtungLive />
    }
}
