use leptos::prelude::*;

#[component]
pub fn UploadForm() -> impl IntoView {
    view! {
        <form>
            <input type="file" name="file" />
            <button type="submit">Upload</button>
        </form>
    }
}
