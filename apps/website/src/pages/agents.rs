use leptos::prelude::*;

#[component]
pub fn Agents() -> impl IntoView {
    view! {
        <h1>"Manage agents"</h1>
        <p>"Upload form will go here"</p>
        <form class="flex flex-col mw-64 space-y-4 p-4 bg-white rounded-lg shadow-md border border-gray-200">
            <label>"GitHub repository URL"</label>
            <input type="text" placeholder="https://github.com/yourname/yourrepo" />

            <label>"Agent name"</label>
            <input type="text" placeholder="Your agent name" />

            <label>"Agent description"</label>
            <textarea placeholder="A short description of your agent"></textarea>

            <label>"Agent version"</label>
            <input type="text" placeholder="1.0.0" />

            <button type="submit">"Upload"</button>
        </form>
    }
}
