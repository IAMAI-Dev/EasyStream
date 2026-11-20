/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        surface: {
          950: "#090b10",
          900: "#0f131c",
          800: "#171d2b",
          700: "#20293d",
        },
      },
    },
  },
  plugins: [],
};
