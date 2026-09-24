export type MockItem = { id: string; weapon: string; skin: string; wear: string; price: number; color: string; rarity: string };
export const items: MockItem[] = [
  { id: 'm4', weapon: 'M4A1-S', skin: 'Decimator', wear: 'Minimal Wear', price: 120, color: '#59d5e5', rarity: 'Restricted' },
  { id: 'awp', weapon: 'AWP', skin: 'Atheris', wear: 'Field-Tested', price: 230, color: '#8b5cf6', rarity: 'Restricted' },
  { id: 'usp', weapon: 'USP-S', skin: 'Cortex', wear: 'Minimal Wear', price: 85, color: '#d67c9d', rarity: 'Classified' },
  { id: 'ak', weapon: 'AK-47', skin: 'Slate', wear: 'Factory New', price: 190, color: '#e6b35b', rarity: 'Restricted' },
  { id: 'glock', weapon: 'Glock-18', skin: 'Water Elemental', wear: 'Field-Tested', price: 160, color: '#5bb6e9', rarity: 'Classified' },
  { id: 'famas', weapon: 'FAMAS', skin: 'Meow 36', wear: 'Minimal Wear', price: 74, color: '#e57e92', rarity: 'Mil-Spec' },
  { id: 'deagle', weapon: 'Desert Eagle', skin: 'Printstream', wear: 'Field-Tested', price: 490, color: '#d9d9df', rarity: 'Covert' },
  { id: 'mp9', weapon: 'MP9', skin: 'Starlight Protector', wear: 'Minimal Wear', price: 112, color: '#b06be8', rarity: 'Classified' },
];
export const results = [items[6], items[1], { ...items[0], id: 'emperor', weapon: 'M4A4', skin: 'The Emperor', price: 2120, color: '#e0a95c', rarity: 'Covert' }, { ...items[2], id: 'neon', weapon: 'AK-47', skin: 'Neon Rider', price: 2680, color: '#ee658a', rarity: 'Covert' }];
