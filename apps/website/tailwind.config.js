/** @type {import('tailwindcss').Config} */
module.exports = {
    content: {
        files: ["*.html", "./src/**/*.rs"],
        transform: {
            rs: (content) => content.replace(/(?:^|\s)class:/g, ' '),
        },
    },
    theme: {
        extend: {
            colors: {
                'primary': '#FF6363',
                'secondary': {
                    100: '#E2E2D5',
                    200: '#888883',
                },
            },
            fontFamily: {
                'body': ['Nunito'],
            },
        },
    },
    plugins: [],
}