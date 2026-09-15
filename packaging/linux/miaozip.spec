Name:           miaozip
Version:        %{miaozip_version}
Release:        1%{?dist}
Summary:        Cross-platform archive manager
License:        MIT
URL:            https://github.com/tangluobo/miaozip
Source0:        miaozip
Source1:        miaozip.desktop
Source2:        miaozip.png
Source3:        README.md
Source4:        LICENSE
Source5:        THIRD_PARTY_NOTICES.md

%description
MiaoZip is a graphical archive manager written in Rust. It can create, browse,
test and extract common archive formats.

%prep

%build

%install
install -Dpm0755 %{SOURCE0} %{buildroot}%{_bindir}/miaozip
install -Dpm0644 %{SOURCE1} %{buildroot}%{_datadir}/applications/miaozip.desktop
install -Dpm0644 %{SOURCE2} %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/miaozip.png
install -Dpm0644 %{SOURCE3} %{buildroot}%{_docdir}/miaozip/README.md
install -Dpm0644 %{SOURCE4} %{buildroot}%{_licensedir}/miaozip/LICENSE
install -Dpm0644 %{SOURCE5} %{buildroot}%{_docdir}/miaozip/THIRD_PARTY_NOTICES.md

%files
%{_bindir}/miaozip
%{_datadir}/applications/miaozip.desktop
%{_datadir}/icons/hicolor/256x256/apps/miaozip.png
%doc %{_docdir}/miaozip/README.md
%doc %{_docdir}/miaozip/THIRD_PARTY_NOTICES.md
%license %{_licensedir}/miaozip/LICENSE

%changelog
* Tue Sep 15 2026 MiaoZip Contributors <noreply@example.com> - %{miaozip_version}-1
- Automated release package
