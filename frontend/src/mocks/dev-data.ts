import type { SkinDefinition } from '../types';

export type MockItem = SkinDefinition;
const IMG: Record<string, string> = {
  m4: 'https://community.akamai.steamstatic.com/economy/image/i0CoZ81Ui0m-9KwlBY1L_18myuGuq1wfhWSaZgMttyVfPaERSR0Wqmu7LAocGIGz3UqlXOLrxM-vMGmW8VNxu5Dx60noTyL8ypexwjFS4_ega6F_H_eAMWrEwL9JtORqRiSygRI1jDGMnYftb3iUb1dxW5ImFLNftxCxktflZLm2tgaP2otGyn_-hytOvy9q5elQV_A7uvqA6CRSoZY',
  awp: 'https://community.akamai.steamstatic.com/economy/image/i0CoZ81Ui0m-9KwlBY1L_18myuGuq1wfhWSaZgMttyVfPaERSR0Wqmu7LAocGIGz3UqlXOLrxM-vMGmW8VNxu5Dx60noTyLwiYbf_jdk7uW-V7JkMPWBMWuZxuZi_rZsS3zgzU8isW3dnIr6eHKfPVAhDpojEe9YsUW4xta1Nuzm5FDci4NbjXKpmWVQppo',
  usp: 'https://community.akamai.steamstatic.com/economy/image/i0CoZ81Ui0m-9KwlBY1L_18myuGuq1wfhWSaZgMttyVfPaERSR0Wqmu7LAocGIGz3UqlXOLrxM-vMGmW8VNxu5Dx60noTyLkjYbf7itX6vytbbZSI-WsG3SA_u1jpN5lRi67gVNz4G7Qm938cS_Da1AhXpB1EeVb4xm4mtDjN7vj4A3b2NpGyCr52i4Y8G81tMzdoYZ7',
  ak: 'https://community.akamai.steamstatic.com/economy/image/i0CoZ81Ui0m-9KwlBY1L_18myuGuq1wfhWSaZgMttyVfPaERSR0Wqmu7LAocGIGz3UqlXOLrxM-vMGmW8VNxu5Dx60noTyLwlcK3wiVI0POlPPNSMOKcCGKD0ud5vuBlcCW6khUz_W3Sytb4cCqTOFUpWJtzTOUD5hPsw9a0Yrnrs1SK3ooXzy6shilM5311o7FVYrIufmI',
  glock: 'https://community.akamai.steamstatic.com/economy/image/i0CoZ81Ui0m-9KwlBY1L_18myuGuq1wfhWSaZgMttyVfPaERSR0Wqmu7LAocGIGz3UqlXOLrxM-vMGmW8VNxu5Dx60noTyL2kpnj9h1Y-s2pZKtuK72fB3aFxP11te99cCW6khUz_TjVyompc3-QOFR2DJQkFOMJtBbqk9LlY-7n5QLZjtkTxCWqhixPv311o7FVIf8eASQ',
  famas: 'https://community.akamai.steamstatic.com/economy/image/i0CoZ81Ui0m-9KwlBY1L_18myuGuq1wfhWSaZgMttyVfPaERSR0Wqmu7LAocGIGz3UqlXOLrxM-vMGmW8VNxu5Dx60noTyL3n5vh7h1c_M2oaalsM8-QAXWA_uNzv_ZWQyC0nQlp6jvVztaudCnEbAUgDsckFOAJsBLtlN2yP7zqslGMiooXyCX43H8Y5zErvbiVlZtU7g',
  deagle: 'https://community.akamai.steamstatic.com/economy/image/i0CoZ81Ui0m-9KwlBY1L_18myuGuq1wfhWSaZgMttyVfPaERSR0Wqmu7LAocGIGz3UqlXOLrxM-vMGmW8VNxu5Dx60noTyL1m5fn8Sdk7OeRbKFsJ8-DHG6e1f1iouRoQha_nBovp3OGmdeqInyVP1V0XsYlRbEI50a5wNyzZr605AyI3t5MmCSohylAuC89_a9cBoMY9UkV',
  mp9: 'https://community.akamai.steamstatic.com/economy/image/i0CoZ81Ui0m-9KwlBY1L_18myuGuq1wfhWSaZgMttyVfPaERSR0Wqmu7LAocGIGz3UqlXOLrxM-vMGmW8VNxu5Dx60noTyL8js_f-jFk4uL3V7d5IeKfB2CY1dF6ueZhW2flkUtztz_SzYypJSqRalUhDJNwQO4PsBXtx9HkN-K37w3bgohGmHn3kGoXuZ3lRdvF',
};
export const items: MockItem[] = [
  { id: 'm4', weapon: 'M4A1-S', skin: 'Decimator', wear: 'Minimal Wear', price: 120, color: '#59d5e5', rarity: 'Restricted', image: IMG.m4 },
  { id: 'awp', weapon: 'AWP', skin: 'Atheris', wear: 'Field-Tested', price: 230, color: '#8b5cf6', rarity: 'Restricted', image: IMG.awp },
  { id: 'usp', weapon: 'USP-S', skin: 'Cortex', wear: 'Minimal Wear', price: 85, color: '#d67c9d', rarity: 'Classified', image: IMG.usp },
  { id: 'ak', weapon: 'AK-47', skin: 'Slate', wear: 'Factory New', price: 190, color: '#e6b35b', rarity: 'Restricted', image: IMG.ak },
  { id: 'glock', weapon: 'Glock-18', skin: 'Water Elemental', wear: 'Field-Tested', price: 160, color: '#5bb6e9', rarity: 'Classified', image: IMG.glock },
  { id: 'famas', weapon: 'FAMAS', skin: 'Meow 36', wear: 'Minimal Wear', price: 74, color: '#e57e92', rarity: 'Mil-Spec', image: IMG.famas },
  { id: 'deagle', weapon: 'Desert Eagle', skin: 'Printstream', wear: 'Field-Tested', price: 490, color: '#d9d9df', rarity: 'Covert', image: IMG.deagle },
  { id: 'mp9', weapon: 'MP9', skin: 'Starlight Protector', wear: 'Minimal Wear', price: 112, color: '#b06be8', rarity: 'Classified', image: IMG.mp9 },
];
export const results = [items[6], items[1], { ...items[0], id: 'emperor', weapon: 'M4A4', skin: 'The Emperor', price: 2120, color: '#e0a95c', rarity: 'Covert' }, { ...items[2], id: 'neon', weapon: 'AK-47', skin: 'Neon Rider', price: 2680, color: '#ee658a', rarity: 'Covert' }];
