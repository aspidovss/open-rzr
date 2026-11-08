// Line drawings of the Naga Pro for the Customize page (own artwork, no Razer images).
// viewBox 0 0 860 420; parts with data-id are clickable buttons.
window.MOUSE_ART = {
  defs: `
    <defs>
      <linearGradient id="g-body" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#2b2b2b"/><stop offset=".55" stop-color="#1c1c1c"/><stop offset="1" stop-color="#121212"/>
      </linearGradient>
      <linearGradient id="g-shell" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#303030"/><stop offset="1" stop-color="#1e1e1e"/>
      </linearGradient>
      <linearGradient id="g-side" x1="0" y1="0" x2="1" y2="0">
        <stop offset="0" stop-color="#151515"/><stop offset=".5" stop-color="#232323"/><stop offset="1" stop-color="#2c2c2c"/>
      </linearGradient>
      <pattern id="p-grip" width="6" height="6" patternUnits="userSpaceOnUse">
        <circle cx="3" cy="3" r="1" fill="#2e2e2e"/>
      </pattern>
    </defs>`,

  // seen from above, front at the top
  top: `
    <path class="art-body" d="M450 26C484 26 514 40 527 72C540 104 547 146 551 196C555 250 549 300 537 336C525 372 495 396 457 397C425 398 397 386 384 352C373 322 378 282 378 248C378 212 368 162 372 114C377 62 410 26 450 26Z" fill="url(#g-body)"/>
    <path class="art-edge" d="M380 206C386 228 388 262 386 300C385 330 390 352 402 368"/>
    <path class="art-edge" d="M544 214C543 266 534 310 518 346"/>
    <path class="mouse-part shell" data-id="1" d="M446 30C414 32 389 48 381 82C374 112 372 150 373 192C399 199 423 203 446 204Z" fill="url(#g-shell)"/>
    <path class="mouse-part shell" data-id="2" d="M454 30C486 32 513 46 523 76C534 104 541 146 546 192C516 200 480 203 454 204Z" fill="url(#g-shell)"/>
    <rect class="art-groove" x="437" y="62" width="26" height="148" rx="13"/>
    <rect class="mouse-part wheel" data-id="3" x="442" y="88" width="16" height="62" rx="8"/>
    <path class="art-rib" d="M444 96H456M444 104H456M444 112H456M444 120H456M444 128H456M444 136H456M444 144H456"/>
    <rect class="mouse-part" data-id="11" x="443" y="163" width="14" height="13" rx="4"/>
    <rect class="mouse-part" data-id="12" x="443" y="181" width="14" height="13" rx="4"/>
    <circle cx="450" cy="352" r="6" class="mouse-part" data-id="14"/>`,

  // thumb buttons seen from above, for mice without a side plate
  topSide: `
    <path class="mouse-part" data-id="5" d="M372 176C368 186 368 198 370 208L378 208C377 198 377 186 379 176Z"/>
    <path class="mouse-part" data-id="4" d="M371 214C369 224 370 236 372 246L380 246C379 236 379 224 379 214Z"/>`,

};
