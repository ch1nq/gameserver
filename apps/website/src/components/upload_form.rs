use leptos::*;

#[component]
pub fn UploadForm() -> impl IntoView {
    view! {
        <form>
            <fieldset style="display: flex; flex-direction: column;">
                <label>
                    First
                    <input name="first_name" placeholder="First name" autocomplete="given-name" />
                </label>
                <label>
                    Email
                    <input type="email" name="email" placeholder="Email" autocomplete="email" />
                </label>
                <label>Source code <input type="file" /></label>
            </fieldset>
        </form>
    }
}
