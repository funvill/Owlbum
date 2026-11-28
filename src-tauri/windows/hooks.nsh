; Owlbum's NSIS installer hooks (docs/08-desktop-and-release.md,
; "Opening pictures"; issue 193). Tauri includes this file near the top of
; its installer template, before any page is declared.
;
; The installer offers "Open pictures with Owlbum" as a checkbox on its
; welcome page, unticked by default, never a silent takeover. Ticked, the
; installer runs `owlbum --associate-pictures` after installing, which
; writes the same per-user registry entries as Settings > Open pictures
; with Owlbum. Silent, passive and update installs skip the welcome page,
; so they never register anything.
;
; The uninstaller runs `owlbum --forget-pictures` first, which removes
; those entries again, so no "Open with" entry points at a removed
; program. An update (/UPDATE) keeps them.

Var OwlbumOpenPictures
Var OwlbumOpenPicturesState

; The template declares the welcome page first, with a PRE function of its
; own and no SHOW or LEAVE function, so these two apply to it.
!define MUI_PAGE_CUSTOMFUNCTION_SHOW OwlbumWelcomeShow
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE OwlbumWelcomeLeave

Function OwlbumWelcomeShow
  ; Below the welcome text, on the page's white background.
  ${NSD_CreateCheckbox} 120u 172u 195u 12u "Open pictures with Owlbum"
  Pop $OwlbumOpenPictures
  SetCtlColors $OwlbumOpenPictures "" "FFFFFF"
  ; Coming back to the page keeps the choice.
  ${If} $OwlbumOpenPicturesState = 1
    ${NSD_Check} $OwlbumOpenPictures
  ${EndIf}
FunctionEnd

Function OwlbumWelcomeLeave
  ${NSD_GetState} $OwlbumOpenPictures $OwlbumOpenPicturesState
FunctionEnd

!macro NSIS_HOOK_POSTINSTALL
  ${If} $OwlbumOpenPicturesState = 1
  ${AndIf} $UpdateMode <> 1
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --associate-pictures'
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --forget-pictures'
  ${EndIf}
!macroend
