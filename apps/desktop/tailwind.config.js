/** @type {import('tailwindcss').Config} */
export default {
    content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
    theme: {
        extend: {
            colors: {
                background: "#0f0f13", // Deep dark background
                surface: "#1c1c21", // Slightly lighter for cards
                primary: "#6366f1", // Indigo/Discord-like
                secondary: "#ec4899", // Pink/Linear-like accent
            },
            fontFamily: {
                sans: ['Inter', 'sans-serif'],
            }
        },
    },
    plugins: [],
}
