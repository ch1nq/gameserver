use crate::components::counter_btn::Button;
use crate::components::page::Page;
use leptos::*;

/// Default Home Page
#[component]
pub fn Home() -> impl IntoView {
    view! {
        <Page>
            <h1>"Achtung battle"</h1>
            <Button />
            <Button increment=5 />
            <progress class="w-1/2" value=50 max=100></progress>
        </Page>
    }
}
