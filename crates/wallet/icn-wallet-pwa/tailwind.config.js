/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    './src/pages/**/*.{js,ts,jsx,tsx,mdx}',
    './src/components/**/*.{js,ts,jsx,tsx,mdx}',
    './src/app/**/*.{js,ts,jsx,tsx,mdx}',
  ],
  theme: {
    extend: {
      colors: {
        'icn-primary': '#4a5568',  // Adjust with actual ICN color scheme
        'icn-secondary': '#2d3748',
        'icn-accent': '#38b2ac',
      },
    },
  },
  plugins: [],
} 