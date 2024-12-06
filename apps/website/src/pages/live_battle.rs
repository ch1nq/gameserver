use leptos::*;

use crate::components::achtung_live::AchtungLive;
use crate::components::page::Page;

#[component]
pub fn LiveBattle() -> impl IntoView {
    view! {
        <Page>
            <h1>"Live battle"</h1>
            <AchtungLive />
        </Page>
    }
}
