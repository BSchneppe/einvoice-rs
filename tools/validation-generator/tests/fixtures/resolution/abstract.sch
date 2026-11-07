<pattern xmlns="http://purl.oclc.org/dsdl/schematron" id="abstract" abstract="true">
  <rule context="$Agreement ">
    <assert id="CII-SR-282" flag="warning" test="$CII-SR-282">Authored buyer diagnostic.</assert>
    <assert id="Other" flag="fatal" test="$Other">Other diagnostic.</assert>
  </rule>
</pattern>
